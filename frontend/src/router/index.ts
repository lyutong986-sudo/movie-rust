import { createRouter, createWebHistory } from 'vue-router';

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'home',
      component: () => import('../pages/HomePage.vue'),
      meta: { title: '首页', section: 'home' }
    },
    {
      path: '/library/:id',
      name: 'library',
      component: () => import('../pages/library/LibraryPage.vue'),
      meta: { section: 'home' }
    },
    {
      path: '/search',
      name: 'search',
      component: () => import('../pages/SearchPage.vue'),
      meta: { title: '搜索', section: 'home' }
    },
    {
      path: '/item/:id',
      name: 'item',
      component: () => import('../pages/item/ItemPage.vue'),
      meta: { title: '详情', section: 'home' }
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('../pages/settings/SettingsIndex.vue'),
      meta: { title: '控制台', section: 'admin', admin: true }
    },
    {
      path: '/settings/server',
      name: 'settings-server',
      component: () => import('../pages/settings/ServerSettings.vue'),
      meta: { title: '服务器', section: 'admin', admin: true }
    },
    {
      path: '/settings/libraries',
      name: 'settings-libraries',
      component: () => import('../pages/settings/LibrarySettings.vue'),
      meta: { title: '媒体库', section: 'admin', admin: true }
    },
    {
      path: '/settings/users',
      name: 'settings-users',
      component: () => import('../pages/settings/UsersSettings.vue'),
      meta: { title: '用户', section: 'admin', admin: true }
    },
    {
      path: '/settings/playback',
      name: 'settings-playback',
      component: () => import('../pages/settings/PlaybackSettings.vue'),
      meta: { title: '播放', section: 'admin', admin: true }
    },
    {
      path: '/settings/network',
      name: 'settings-network',
      component: () => import('../pages/settings/NetworkSettings.vue'),
      meta: { title: '网络', section: 'admin', admin: true }
    },
    {
      path: '/server/login',
      name: 'server-login',
      component: () => import('../pages/server/LoginPage.vue'),
      meta: { title: '登录', layout: 'server' }
    },
    {
      path: '/wizard',
      name: 'wizard',
      component: () => import('../pages/WizardPage.vue'),
      meta: { title: '首次启动', layout: 'server' }
    }
  ]
});
