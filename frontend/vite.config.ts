import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import ui from '@nuxt/ui/vite';

// 代理所有 Emby 兼容端点到后端，保持之前的路径，避免重复造轮子。
export default defineConfig({
  plugins: [
    vue(),
    ui({
      ui: {
        colors: {
          primary: 'sky',
          neutral: 'slate'
        }
      }
    })
  ],
  server: {
    proxy: {
      '/System': 'http://127.0.0.1:8096',
      '/Startup': 'http://127.0.0.1:8096',
      '/Users': 'http://127.0.0.1:8096',
      '/Items': 'http://127.0.0.1:8096',
      '/Library': 'http://127.0.0.1:8096',
      '/Environment': 'http://127.0.0.1:8096',
      '/Videos': 'http://127.0.0.1:8096',
      '/Sessions': 'http://127.0.0.1:8096',
      '/PlayingItems': 'http://127.0.0.1:8096',
      '/UserItems': 'http://127.0.0.1:8096',
      '/UserFavoriteItems': 'http://127.0.0.1:8096',
      '/UserPlayedItems': 'http://127.0.0.1:8096',
      '/Branding': 'http://127.0.0.1:8096',
      '/api': 'http://127.0.0.1:8096'
    }
  },
  // 把大体积第三方库单独切 chunk，主 bundle 从 ~590KB 降到 <200KB。
  build: {
    chunkSizeWarningLimit: 800,
    rollupOptions: {
      output: {
        manualChunks: {
          vue: ['vue', 'vue-router'],
          'nuxt-ui': ['@nuxt/ui/vue-plugin']
        }
      }
    }
  }
});
