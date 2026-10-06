import { useCallback, useEffect, useRef, useState } from 'react';
import {
  Alert, Button, Divider, Input, Layout, Modal, Select, Space, Spin, Table, Tree, Typography,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { http } from '../api/http';

const { TextArea } = Input;
const { Sider, Content } = Layout;

type Cell = string | number | boolean | null;
type Row = Cell[];
interface QueryResult {
  success: boolean;
  columns: { name: string; typeName: string; nullable: boolean }[];
  rows: Row[];
  total: number;
  page: number;
  pageSize: number;
  hasMore: boolean;
  execMs: number;
  affectedRows?: number | null;
  needConfirm?: string | null;
}
interface Connection {
  id: number;
  name: string;
  connType: string;
  readOnlyLock?: boolean;
}

export default function QueryPage() {
  const [conns, setConns] = useState<Connection[]>([]);
  const [connId, setConnId] = useState<number | null>(null);
  const [databases, setDatabases] = useState<string[]>([]);
  const [tables, setTables] = useState<{ name: string }[]>([]);
  const [treeLoading, setTreeLoading] = useState(false);
  const [sql, setSql] = useState('SELECT ...');
  const [executing, setExecuting] = useState(false);
  const [result, setResult] = useState<QueryResult | null>(null);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(50);
  const [confirmVisible, setConfirmVisible] = useState(false);
  const [confirmReason, setConfirmReason] = useState('');
  const pendingSql = useRef('');
  const [exporting, setExporting] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const r = await http.get('/connections');
        const items = (r.data as any).data.items as Connection[];
        setConns(items);
        const first = items.find((c: Connection) => c.readOnlyLock === false) ?? items[0];
        if (first) pickConn(first.id);
      } catch (e: any) { message.error(e?.response?.data?.message ?? '加载连接失败'); }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const pickConn = useCallback(async (id: number) => {
    setConnId(id);
    setDatabases([]); setTables([]); setResult(null);
    try {
      setTreeLoading(true);
      const r = await http.post('/meta/databases', { connectionId: id });
      setDatabases((r.data as any).data.items as string[]);
    } catch (e: any) { message.error(e?.response?.data?.message ?? '读库失败'); }
    finally { setTreeLoading(false); }
  }, []);

  const pickDb = useCallback(async (db: string) => {
    if (!connId) return;
    try {
      setTreeLoading(true);
      const r = await http.post('/meta/tables', { connectionId: connId, database: db });
      setTables((r.data as any).data.items as { name: string }[]);
    } catch (e: any) { message.error(e?.response?.data?.message ?? '读表失败'); }
    finally { setTreeLoading(false); }
  }, [connId]);

  const openTable = (table: string) => {
    const db = databases[0];
    const q = db ? `SELECT * FROM "${db}"."${table}" LIMIT 100` : `SELECT * FROM ${table} LIMIT 100`;
    setSql(q);
    runSql(q, pageSize, 1, false);
  };

  const runSql = useCallback(async (text: string, size: number, pg: number, confirmed: boolean) => {
    if (!connId) { message.warning('请先选择连接'); return; }
    if (!text.trim()) { message.warning('请输入 SQL'); return; }
    setExecuting(true);
    try {
      const body: any = { connectionId: connId, sql: text, page: pg, pageSize: size, timeoutMs: 120000 };
      if (confirmed) body.confirm = true;
      const r = await http.post('/query', body);
      const res = (r.data as any).data as QueryResult;
      if (res.needConfirm) {
        pendingSql.current = text;
        setConfirmReason(res.needConfirm);
        setConfirmVisible(true);
        setResult(res);
      } else {
        setResult(res);
        setPage(pg);
      }
    } catch (e: any) {
      message.error(e?.response?.data?.message ?? '执行失败');
    } finally { setExecuting(false); }
  }, [connId]);

  const doExecute = () => runSql(sql, pageSize, page, false);
  const changePage = (p: number, s: number) => { setPageSize(s); runSql(sql, s, p, false); };

  const doExport = async (format: string) => {
    if (!connId) return;
    setExporting(true);
    try {
      const r = await http.post('/export', { connectionId: connId, sql, format, maxRows: 100000 });
      const file = (r.data as any).data.file as string;
      const dl = await http.get('/export/download', { params: { path: file }, responseType: 'blob' });
      const url = URL.createObjectURL(dl.data as Blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = file.split('/').pop() ?? 'export';
      a.click();
      URL.revokeObjectURL(url);
      message.success(`已导出 ${file}`);
    } catch (e: any) {
      message.error(e?.response?.data?.message ?? '导出失败');
    } finally { setExporting(false); }
  };

  const columns: ColumnsType<any> = (result?.columns ?? []).map((c) => ({
    title: c.name,
    dataIndex: c.name,
    key: c.name,
    ellipsis: true,
    render: (v: Cell) => (v === null ? <Typography.Text type="secondary">NULL</Typography.Text> : String(v)),
  }));
  const dataSource = (result?.rows ?? []).map((r, i) => {
    const obj: any = { key: i };
    (result!.columns).forEach((c, ci) => { obj[c.name] = r[ci]; });
    return obj;
  });

  return (
    <Layout style={{ height: '100vh' }}>
      <Sider width={260} theme="light" style={{ borderRight: '1px solid #f0f0f0', padding: 12, overflow: 'auto' }}>
        <Space direction="vertical" style={{ width: '100%' }}>
          <Typography.Text strong>连接</Typography.Text>
          <Select
            style={{ width: '100%' }} value={connId ?? undefined} placeholder="选择连接"
            onChange={pickConn}
            options={conns.map((c) => ({ value: c.id, label: `${c.name}（${c.connType}）` }))}
          />
          <Divider style={{ margin: '8px 0' }} />
          <Typography.Text strong>数据库 / 表</Typography.Text>
          {treeLoading ? <Spin size="small" /> : (
            <Tree
              treeData={databases.map((db) => ({
                key: db, title: db,
                isLeaf: false,
                children: tables.length && db === databases[0]
                  ? tables.map((t) => ({ key: `${db}.${t.name}`, title: t.name, isLeaf: true }))
                  : undefined,
              }))}
              loadData={node => { if (node.key === databases[0]) return pickDb(String(node.key)); return Promise.resolve(); }}
              onSelect={(keys) => {
                const k = String(keys[0] ?? '');
                if (k.includes('.')) openTable(k.split('.')[1]);
              }}
            />
          )}
        </Space>
      </Sider>
      <Content style={{ padding: 16, display: 'flex', flexDirection: 'column' }}>
        <Space style={{ marginBottom: 8 }}>
          <Select
            value={pageSize} style={{ width: 110 }} onChange={setPageSize}
            options={[50, 100, 500, 1000].map((n) => ({ value: n, label: `${n} 行/页` }))}
          />
          <Button type="primary" loading={executing} onClick={doExecute}>执行</Button>
          <Button loading={exporting} onClick={() => doExport('csv')}>导出 CSV</Button>
          <Button onClick={() => doExport('xlsx')}>导出 XLSX</Button>
          <Button onClick={() => doExport('json')}>导出 JSON</Button>
          <Button onClick={() => doExport('sql')}>导出 SQL</Button>
        </Space>
        <TextArea
          value={sql} onChange={(e) => setSql(e.target.value)} autoSize={{ minRows: 4, maxRows: 8 }}
          style={{ fontFamily: 'monospace' }}
          placeholder="输入 SQL（写操作需二次确认；生产只读锁连接禁止写）"
        />
        {result?.affectedRows !== null && result?.affectedRows !== undefined && (
          <Alert style={{ marginTop: 8 }} type="success" showIcon
            message={`执行成功：影响 ${result.affectedRows} 行，耗时 ${result.execMs}ms`} />
        )}
        {result?.success && !result.affectedRows && (
          <Alert style={{ marginTop: 8 }} type="info" showIcon
            message={`${result.total} 行 · ${result.execMs}ms${result.hasMore ? ' · 还有更多（下一页）' : ''}`} />
        )}
        <div style={{ flex: 1, marginTop: 8, overflow: 'auto' }}>
          <Table
            size="small" bordered columns={columns} dataSource={dataSource}
            pagination={{
              current: page, pageSize, total: result?.total ?? dataSource.length,
              showSizeChanger: false, onChange: changePage,
            }}
            scroll={{ x: 'max-content' }}
          />
        </div>
      </Content>
      <Modal
        open={confirmVisible}
        title="危险操作确认（D6 二次确认）"
        okText="确认执行"
        okButtonProps={{ danger: true }}
        cancelText="取消"
        onOk={async () => {
          setConfirmVisible(false);
          await runSql(pendingSql.current, pageSize, page, true);
        }}
        onCancel={() => { setConfirmVisible(false); setResult(null); }}
      >
        <p>{confirmReason}</p>
      </Modal>
    </Layout>
  );
}
