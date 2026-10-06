import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// 开发期把 /api 代理到 dbloom-server（8081）；生产由同一镜像/网关反代。
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8081',
        changeOrigin: true,
      },
    },
  },
});
