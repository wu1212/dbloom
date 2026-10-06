import { useState } from 'react';
import { Button, Card, Form, Input, Typography, message, Alert } from 'antd';
import { useNavigate } from 'react-router-dom';
import { useAuthStore } from '../store/auth';

export default function LoginPage() {
  const navigate = useNavigate();
  const login = useAuthStore((s) => s.login);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string>('');

  const onFinish = async (v: { username: string; password: string }) => {
    setLoading(true);
    setErr('');
    try {
      const user = await login(v.username, v.password);
      message.success(`欢迎，${user.displayName ?? user.username}`);
      navigate('/', { replace: true });
    } catch (e: any) {
      setErr(e?.response?.data?.message ?? '登录失败，请检查用户名密码');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ minHeight: '100vh', display: 'flex', alignItems: 'center', justifyContent: 'center', background: '#f5f5f5' }}>
      <Card style={{ width: 380 }}>
        <Typography.Title level={3} style={{ textAlign: 'center', marginTop: 0 }}>
          dbloom
        </Typography.Title>
        <Typography.Paragraph type="secondary" style={{ textAlign: 'center' }}>
          数据同步与数据库客户端
        </Typography.Paragraph>
        {err && <Alert type="error" message={err} showIcon style={{ marginBottom: 12 }} />}
        <Form layout="vertical" onFinish={onFinish}>
          <Form.Item label="用户名" name="username" rules={[{ required: true, message: '请输入用户名' }]}>
            <Input placeholder="admin" autoComplete="username" />
          </Form.Item>
          <Form.Item label="密码" name="password" rules={[{ required: true, message: '请输入密码' }]}>
            <Input.Password placeholder="******" autoComplete="current-password" />
          </Form.Item>
          <Button type="primary" htmlType="submit" block loading={loading}>
            登录
          </Button>
        </Form>
      </Card>
    </div>
  );
}
