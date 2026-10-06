import { BrowserRouter, Routes, Route, Navigate, Link } from 'react-router-dom';
import { useAuthStore } from './store/auth';
import LoginPage from './pages/LoginPage';
import ConnectionsPage from './pages/ConnectionsPage';
import QueryPage from './pages/QueryPage';
import { Layout, Menu } from 'antd';

function RequireAuth({ children }: { children: React.ReactNode }) {
  const accessToken = useAuthStore((s) => s.accessToken);
  if (!accessToken) return <Navigate to="/login" replace />;
  return <>{children}</>;
}

function Shell({ children }: { children: React.ReactNode }) {
  const user = useAuthStore((s) => s.user);
  const clear = useAuthStore((s) => s.clear);
  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Layout.Header style={{ display: 'flex', alignItems: 'center' }}>
        <div style={{ color: '#fff', fontWeight: 600, marginRight: 32 }}>dbloom</div>
        <Menu
          theme="dark" mode="horizontal" style={{ flex: 1 }}
          items={[
            { key: 'conn', label: <Link to="/">连接管理</Link> },
            { key: 'query', label: <Link to="/query">SQL 工作台</Link> },
          ]}
        />
        <span style={{ color: '#aaa', marginRight: 16 }}>
          {user?.username}{user?.role === 'admin' ? '（管理员）' : ''}
        </span>
        <a style={{ color: '#fff' }} onClick={() => clear()}>退出</a>
      </Layout.Header>
      <Layout.Content>{children}</Layout.Content>
    </Layout>
  );
}

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route path="/" element={<RequireAuth><Shell><ConnectionsPage /></Shell></RequireAuth>} />
        <Route path="/query" element={<RequireAuth><Shell><QueryPage /></Shell></RequireAuth>} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}
