import axios from 'axios';
import { useAuthStore } from '../store/auth';

// 统一 axios 实例：自动带 Bearer；401（access 过期）时尝试 refresh，失败回登录页。
export const http = axios.create({ baseURL: '/api/v1', timeout: 15000 });

export interface ApiEnvelope<T> {
  code: number;
  data: T;
  message?: string;
  trace_id?: string;
}

http.interceptors.request.use((config) => {
  const token = useAuthStore.getState().accessToken;
  if (token) config.headers.Authorization = `Bearer ${token}`;
  return config;
});

let refreshing: Promise<boolean> | null = null;

async function tryRefresh(): Promise<boolean> {
  const { refreshToken, setSession, clear } = useAuthStore.getState();
  if (!refreshToken) return false;
  try {
    const resp = await axios.post<ApiEnvelope<{ accessToken: string }>>('/api/v1/auth/refresh', {
      refreshToken,
    });
    setSession({ accessToken: resp.data.data.accessToken, refreshToken });
    return true;
  } catch {
    clear();
    return false;
  }
}

http.interceptors.response.use(
  (r) => r,
  async (error) => {
    const original = error.config as any;
    const status = error.response?.status;
    if (status === 401 && !original?._retried) {
      original._retried = true;
      refreshing = refreshing ?? tryRefresh();
      const ok = await refreshing;
      refreshing = null;
      if (ok) return http(original);
    }
    return Promise.reject(error);
  },
);
