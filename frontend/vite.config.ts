import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

export default defineConfig({
  plugins: [vue()],
  server: {
    proxy: {
      '/System': 'http://127.0.0.1:8096',
      '/Startup': 'http://127.0.0.1:8096',
      '/Users': 'http://127.0.0.1:8096',
      '/Items': 'http://127.0.0.1:8096',
      '/Library': 'http://127.0.0.1:8096',
      '/Videos': 'http://127.0.0.1:8096',
      '/Sessions': 'http://127.0.0.1:8096',
      '/PlayingItems': 'http://127.0.0.1:8096',
      '/UserItems': 'http://127.0.0.1:8096',
      '/UserFavoriteItems': 'http://127.0.0.1:8096',
      '/UserPlayedItems': 'http://127.0.0.1:8096',
      '/Branding': 'http://127.0.0.1:8096',
      '/api': 'http://127.0.0.1:8096'
    }
  }
});
