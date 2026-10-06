import { useCallback, useEffect, useState } from 'react';
import {
  Button, Form, Input, InputNumber, Modal, Select, Space, Table, Tag, Typography, message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { http } from '../api/http';
import { useAuthStore } from '../store/auth';

// ---------- 类型（与后端 DTO 对齐，serde camelCase） ----------
interface FormField {
  key: string;
  label: string;
  fieldType: string;
  required: boolean;
  default?: string | null;
  placeholder?: string | null;
  options: string[];
}
interface ConnTypeManifest {
  name: string;
  label: string;
  kind: string;
  implemented: boolean;
  formFields: FormField[];
}
interface Connection {
  id: number;
  name: string;
  connType: string;
  host: string;
  port?: number | null;
  databaseName?: string | null;
  username?: string | null;
  sslMode: string;
  isProduction: boolean;
  readOnlyLock: boolean;
  createdAt: number;
}

interface ConnFormValues {
  connType: string;
  name: string;
  host: string;
  [k: string]: any;
}

// ---------- Manifest 驱动表单 ----------
function ConnForm({ types, initial, onSaved }: { types: ConnTypeManifest[]; initial?: Connection | null; onSaved: () => void }) {
  const [form] = Form.useForm<ConnFormValues>();
  const [saving, setSaving] = useState(false);
  const connType = Form.useWatch('connType', form);
  const manifest = types.find((t) => t.name === connType);

  const renderField = (f: FormField) => {
    const common = {
      key: f.key,
      label: f.label,
      rules: f.required ? [{ required: true, message: `请填写${f.label}` }] : undefined,
    };
    if (f.fieldType === 'number') {
      return (
        <Form.Item {...common} name={f.key}>
          <InputNumber style={{ width: '100%' }} placeholder={f.placeholder ?? ''} />
        </Form.Item>
      );
    }
    if (f.fieldType === 'password') {
      return (
        <Form.Item {...common} name={f.key}>
          <Input.Password placeholder={initial ? '（留空不修改）' : (f.placeholder ?? '')} autoComplete="new-password" />
        </Form.Item>
      );
    }
    if (f.fieldType === 'select') {
      return (
        <Form.Item {...common} name={f.key}>
          <Select allowClear options={f.options.map((o) => ({ value: o, label: o }))} />
        </Form.Item>
      );
    }
    return (
      <Form.Item {...common} name={f.key}>
        <Input placeholder={f.placeholder ?? ''} />
      </Form.Item>
    );
  };

  const submit = async (v: ConnFormValues) => {
    setSaving(true);
    try {
      const payload: Record<string, any> = { ...v };
      if (payload.password === undefined) delete payload.password;
      if (initial) {
        await http.put(`/connections/${initial.id}`, payload);
        message.success('连接已更新');
      } else {
        await http.post('/connections', payload);
        message.success('连接已创建');
      }
      onSaved();
    } catch (e: any) {
      message.error(e?.response?.data?.message ?? '保存失败');
    } finally {
      setSaving(false);
    }
  };

  return (
    <Form
      form={form}
      layout="vertical"
      initialValues={
        initial
          ? {
              connType: initial.connType,
              name: initial.name,
              host: initial.host,
              port: initial.port,
              databaseName: initial.databaseName,
              username: initial.username,
              sslMode: initial.sslMode,
            }
          : { connType: types[0]?.name }
      }
      onFinish={submit}
    >
      <Form.Item label="连接类型" name="connType" rules={[{ required: true, message: '请选择类型' }]}>
        <Select
          options={types.map((t) => ({
            value: t.name,
            label: `${t.label}${t.implemented ? '' : '（驱动未就绪）'}`,
          }))}
        />
      </Form.Item>
      <Form.Item label="连接名称" name="name" rules={[{ required: true, message: '请输入名称' }]}>
        <Input placeholder="如 生产MySQL-1" />
      </Form.Item>
      {manifest?.formFields?.map(renderField)}
      <Space>
        <Button type="primary" htmlType="submit" loading={saving}>
          {initial ? '保存修改' : '创建连接'}
        </Button>
        {initial && (
          <Button onClick={() => form.setFieldsValue({ password: undefined })}>清除密码输入</Button>
        )}
      </Space>
    </Form>
  );
}

// ---------- 列表页 ----------
export default function ConnectionsPage() {
  const user = useAuthStore((s) => s.user);
  const clear = useAuthStore((s) => s.clear);
  const [types, setTypes] = useState<ConnTypeManifest[]>([]);
  const [rows, setRows] = useState<Connection[]>([]);
  const [loading, setLoading] = useState(false);
  const [openCreate, setOpenCreate] = useState(false);
  const [editing, setEditing] = useState<Connection | null>(null);
  const [testing, setTesting] = useState<number | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [t, c] = await Promise.all([
        http.get('/connections/types'),
        http.get('/connections'),
      ]);
      setTypes(t.data.data);
      setRows(c.data.data.items);
    } catch (e: any) {
      message.error(e?.response?.data?.message ?? '加载连接失败');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const testConn = async (id: number) => {
    setTesting(id);
    try {
      const r = await http.post(`/connections/${id}/test`);
      const d = r.data.data;
      if (d.success) message.success(`连接成功（${d.latencyMs}ms）`);
      else message.error(`连接失败：${d.message}`);
    } catch (e: any) {
      message.error(e?.response?.data?.message ?? '测试失败');
    } finally {
      setTesting(null);
    }
  };

  const toggleLock = async (row: Connection) => {
    try {
      await http.post(`/connections/${row.id}/${row.readOnlyLock ? 'unlock' : 'lock'}`, { confirm: true });
      message.success(row.readOnlyLock ? '已解除只读锁' : '已启用只读锁');
      load();
    } catch (e: any) {
      message.error(e?.response?.data?.message ?? '操作失败');
    }
  };

  const delConn = async (row: Connection) => {
    Modal.confirm({
      title: `删除连接「${row.name}」？`,
      content: '删除为软删除，可从审计追踪；被任务引用的连接需先解除引用。',
      onOk: async () => {
        await http.delete(`/connections/${row.id}`);
        message.success('已删除');
        load();
      },
    });
  };

  const columns: ColumnsType<Connection> = [
    { title: '名称', dataIndex: 'name' },
    {
      title: '类型',
      dataIndex: 'connType',
      render: (v: string) => <Tag color={v === 'mysql' ? 'blue' : v === 'postgres' ? 'geekblue' : 'default'}>{v}</Tag>,
    },
    { title: '主机', dataIndex: 'host' },
    { title: '端口', dataIndex: 'port', width: 80 },
    { title: '库/索引', dataIndex: 'databaseName' },
    { title: '用户', dataIndex: 'username' },
    {
      title: '生产',
      dataIndex: 'isProduction',
      width: 80,
      render: (v: boolean) => (v ? <Tag color="red">生产</Tag> : <Tag>开发</Tag>),
    },
    {
      title: '只读锁',
      dataIndex: 'readOnlyLock',
      width: 90,
      render: (v: boolean) => (v ? <Tag color="volcano">锁定</Tag> : <Tag>正常</Tag>),
    },
    {
      title: '操作',
      width: 260,
      render: (_, row) => (
        <Space>
          <Button size="small" loading={testing === row.id} onClick={() => testConn(row.id)}>
            测试
          </Button>
          <Button size="small" onClick={() => setEditing(row)}>
            编辑
          </Button>
          <Button size="small" danger={row.readOnlyLock} onClick={() => toggleLock(row)}>
            {row.readOnlyLock ? '解锁' : '只读锁'}
          </Button>
          <Button size="small" danger onClick={() => delConn(row)}>
            删除
          </Button>
        </Space>
      ),
    },
  ];

  return (
    <div style={{ padding: 24 }}>
      <Space style={{ width: '100%', justifyContent: 'space-between', marginBottom: 16 }}>
        <Typography.Title level={4} style={{ margin: 0 }}>
          数据库连接
        </Typography.Title>
        <Space>
          <Typography.Text type="secondary">
            {user?.username}（{user?.role === 'admin' ? '管理员' : '普通用户'}）
          </Typography.Text>
          <Button onClick={() => { clear(); }}>退出</Button>
          <Button type="primary" onClick={() => { setEditing(null); setOpenCreate(true); }}>
            新建连接
          </Button>
        </Space>
      </Space>

      <Table rowKey="id" loading={loading} columns={columns} dataSource={rows} pagination={{ pageSize: 10 }} />

      <Modal
        title={editing ? `编辑连接：${editing.name}` : '新建连接'}
        open={openCreate || !!editing}
        footer={null}
        width={520}
        destroyOnClose
        onCancel={() => { setOpenCreate(false); setEditing(null); }}
      >
        {types.length > 0 && (
          <ConnForm
            types={types}
            initial={editing}
            onSaved={() => {
              setOpenCreate(false);
              setEditing(null);
              load();
            }}
          />
        )}
      </Modal>
    </div>
  );
}
