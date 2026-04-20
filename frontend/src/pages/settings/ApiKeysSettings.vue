<script setup lang="ts">
import SettingsNav from '../../components/SettingsNav.vue';
import { api, isAdmin } from '../../store/app';
</script>

<template>
  <section class="settings-shell">
    <SettingsNav />

    <div class="settings-content">
      <div v-if="!isAdmin" class="empty">
        <p>API Key</p>
        <h2>需要管理员权限</h2>
        <p>当前账户不能查看令牌兼容说明。</p>
      </div>

      <div v-else class="settings-page">
        <div class="placeholder-grid">
          <article>
            <h3>当前访问令牌</h3>
            <p>当前版本优先兼容 Emby/Jellyfin 登录令牌，而不是独立 API Key 管理页。</p>
            <code class="token-box mono">{{ api.token || '当前没有令牌' }}</code>
          </article>
          <article>
            <h3>兼容方式</h3>
            <p>服务端已支持 `X-Emby-Token`、`X-Emby-Authorization`、`Authorization` 和 `api_key`。</p>
          </article>
          <article>
            <h3>后续计划</h3>
            <p>下一步可以继续补独立 API Key 生成、撤销与用途标记，使它更贴近 Jellyfin 后台逻辑。</p>
          </article>
        </div>
      </div>
    </div>
  </section>
</template>
