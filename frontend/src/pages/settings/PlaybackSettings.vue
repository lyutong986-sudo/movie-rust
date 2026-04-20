<script setup lang="ts">
import SettingsNav from '../../components/SettingsNav.vue';
import { isAdmin } from '../../store/app';
</script>

<template>
  <section class="settings-shell">
    <SettingsNav />

    <div class="settings-content">
      <div v-if="!isAdmin" class="empty">
        <p>播放</p>
        <h2>需要管理员权限</h2>
        <p>当前账户不能查看播放器兼容配置。</p>
      </div>

      <div v-else class="settings-page">
        <div class="stat-grid">
          <article>
            <small>认证方式</small>
            <strong>X-Emby-Authorization</strong>
            <span>兼容 Emby 客户端登录头</span>
          </article>
          <article>
            <small>播放模式</small>
            <strong>Direct Stream</strong>
            <span>优先直链，保留 Emby/本地播放器接入</span>
          </article>
          <article>
            <small>字幕来源</small>
            <strong>外挂字幕</strong>
            <span>自动识别 sidecar subtitle 文件</span>
          </article>
        </div>

        <div class="placeholder-grid">
          <article>
            <h3>本地播放器联动</h3>
            <p>服务端已兼容 Emby 风格登录和播放流地址，适合直接接入你现有的本地播放器。</p>
          </article>
          <article>
            <h3>播放进度回传</h3>
            <p>已支持 Favorite、Played、UserData 更新接口，可继续扩展进度同步和会话控制。</p>
          </article>
          <article>
            <h3>详情页轨道选择</h3>
            <p>前端详情页已能读取媒体源、音轨、字幕轨信息，后续可以继续对齐 Jellyfin 的播放器页。</p>
          </article>
        </div>
      </div>
    </div>
  </section>
</template>
