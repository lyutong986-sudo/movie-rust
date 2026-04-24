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
      meta: { title: '媒体库', section: 'home' }
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
      path: '/series/:id',
      name: 'series',
      component: () => import('../pages/series/SeriesPage.vue'),
      meta: { title: '剧集', section: 'home' }
    },
    {
      path: '/genre/:id',
      name: 'genre',
      component: () => import('../pages/genre/GenrePage.vue'),
      meta: { title: '类型', section: 'home' }
    },
    {
      path: '/playback/video',
      name: 'playback-video',
      component: () => import('../pages/playback/VideoPlaybackPage.vue'),
      meta: { title: '视频播放', layout: 'fullpage' }
    },
    {
      path: '/playback/music',
      name: 'playback-music',
      component: () => import('../pages/playback/MusicPlaybackPage.vue'),
      meta: { title: '音乐播放', layout: 'fullpage' }
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('../pages/settings/SettingsIndex.vue'),
      meta: { title: '设置', section: 'settings' }
    },
    {
      path: '/settings/account',
      name: 'settings-account',
      component: () => import('../pages/settings/AccountSettings.vue'),
      meta: { title: '账户', section: 'settings' }
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
      path: '/settings/remote-emby',
      name: 'settings-remote-emby',
      component: () => import('../pages/settings/RemoteEmbySettings.vue'),
      meta: { title: '远端 Emby 中转', section: 'admin', admin: true }
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
      meta: { title: '播放', section: 'settings' }
    },
    {
      path: '/settings/transcoding',
      name: 'settings-transcoding',
      component: () => import('../pages/settings/TranscodingSettings.vue'),
      meta: { title: '转码', section: 'admin', admin: true }
    },
    {
      path: '/settings/subtitles',
      name: 'settings-subtitles',
      component: () => import('../pages/settings/SubtitlesSettings.vue'),
      meta: { title: '字幕', section: 'settings' }
    },
    {
      path: '/settings/devices',
      name: 'settings-devices',
      component: () => import('../pages/settings/DevicesSettings.vue'),
      meta: { title: '设备', section: 'admin', admin: true }
    },
    {
      path: '/settings/apikeys',
      name: 'settings-apikeys',
      component: () => import('../pages/settings/ApiKeysSettings.vue'),
      meta: { title: 'API Key', section: 'admin', admin: true }
    },
    {
      path: '/settings/logs-and-activity',
      name: 'settings-logs-and-activity',
      component: () => import('../pages/settings/LogsActivitySettings.vue'),
      meta: { title: '日志与活动', section: 'admin', admin: true }
    },
    {
      path: '/settings/network',
      name: 'settings-network',
      component: () => import('../pages/settings/NetworkSettings.vue'),
      meta: { title: '网络', section: 'admin', admin: true }
    },
    {
      path: '/settings/scheduled-tasks',
      name: 'settings-scheduled-tasks',
      component: () => import('../pages/settings/ScheduledTasksSettings.vue'),
      meta: { title: '计划任务', section: 'admin', admin: true }
    },
    {
      path: '/settings/subtitle-download',
      name: 'settings-subtitle-download',
      component: () => import('../pages/settings/SubtitleDownloadSettings.vue'),
      meta: { title: '字幕下载', section: 'admin', admin: true }
    },
    {
      path: '/settings/library-display',
      name: 'settings-library-display',
      component: () => import('../pages/settings/LibraryDisplaySettings.vue'),
      meta: { title: '媒体库显示', section: 'admin', admin: true }
    },
    {
      path: '/settings/branding',
      name: 'settings-branding',
      component: () => import('../pages/settings/BrandingSettings.vue'),
      meta: { title: '品牌化', section: 'admin', admin: true }
    },
    {
      path: '/settings/reports',
      name: 'settings-reports',
      component: () => import('../pages/settings/ReportsSettings.vue'),
      meta: { title: '活动报表', section: 'admin', admin: true }
    },
    {
      path: '/playlists',
      name: 'playlists',
      component: () => import('../pages/PlaylistsPage.vue'),
      meta: { title: '播放列表', section: 'home' }
    },
    {
      path: '/playlist/:id',
      name: 'playlist-detail',
      component: () => import('../pages/PlaylistDetailPage.vue'),
      meta: { title: '播放列表详情', section: 'home' }
    },
    {
      path: '/server/forgot-password',
      name: 'server-forgot-password',
      component: () => import('../pages/server/ForgotPasswordPage.vue'),
      meta: { title: '找回密码', layout: 'server' }
    },
    {
      path: '/server/login',
      name: 'server-login',
      component: () => import('../pages/server/LoginPage.vue'),
      meta: { title: '登录', layout: 'server' }
    },
    {
      path: '/server/select',
      name: 'server-select',
      component: () => import('../pages/server/SelectServerPage.vue'),
      meta: { title: '选择服务器', layout: 'server' }
    },
    {
      path: '/server/add',
      name: 'server-add',
      component: () => import('../pages/server/AddServerPage.vue'),
      meta: { title: '添加服务器', layout: 'server' }
    },
    {
      path: '/wizard',
      name: 'wizard',
      component: () => import('../pages/WizardPage.vue'),
      meta: { title: '首次启动', layout: 'server' }
    },
    {
      path: '/queue',
      name: 'queue',
      component: () => import('../pages/QueuePage.vue'),
      meta: { title: '播放队列 / 稍后观看', section: 'home' }
    },
    {
      path: '/:pathMatch(.*)*',
      name: 'not-found',
      component: () => import('../pages/NotFoundPage.vue'),
      meta: { title: '找不到页面' }
    }
  ]
});
