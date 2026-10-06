import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { ApiEnvelope } from '../api/http';
import { http } from '../api/http';

export interface UserInfo {
  id: number;
  username: string;
  role: string;
  displayName?: string | null;
}

interface AuthState {
  accessToken: string | null;
  refreshToken: string | null;
  user: UserInfo | null;
  setSession: (s: { accessToken: string; refreshToken: string; user?: UserInfo }) => void;
  clear: () => void;
  login: (username: string, password: string) => Promise<UserInfo>;
}

interface LoginResp {
  accessToken: string;
  refreshToken: string;
  user: UserInfo;
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      accessToken: null,
      refreshToken: null,
      user: null,
      setSession: (s) => set({ ...s }),
      clear: () => set({ accessToken: null, refreshToken: null, user: null }),
      login: async (username, password) => {
        const resp = await http.post<ApiEnvelope<LoginResp>>('/auth/login', { username, password });
        const d = resp.data.data;
        get().setSession({ accessToken: d.accessToken, refreshToken: d.refreshToken, user: d.user });
        return d.user;
      },
    }),
    { name: 'dbloom-auth' },
  ),
);
