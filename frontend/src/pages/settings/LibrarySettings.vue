<script setup lang="ts">
import { onMounted } from 'vue';
import AddLibraryDialog from '../../components/AddLibraryDialog.vue';
import SettingsNav from '../../components/SettingsNav.vue';
import { isAdmin, libraries, loadAdminData, loadLibraries, scan, state } from '../../store/app';

onMounted(async () => {
  if (isAdmin.value) {
    await Promise.all([loadAdminData(), loadLibraries()]);
  }
});
</script>

<template>
  <section class="settings-shell">
    <SettingsNav />

    <div class="settings-content">
      <div v-if="!isAdmin" class="empty">
        <p>媒体库</p>
        <h2>需要管理员权限</h2>
        <p>当前账户不能创建媒体库或触发扫描。</p>
      </div>

      <div v-else class="settings-page">
        <div class="button-row">
          <button type="button" @click="state.showAddLibrary = true">添加媒体库</button>
          <button class="secondary" :disabled="state.busy" type="button" @click="scan">执行扫描</button>
        </div>

        <div v-if="libraries.length" class="user-admin-grid">
          <article v-for="library in libraries" :key="library.Id">
            <span>{{ library.CollectionType === 'tvshows' ? '剧' : library.CollectionType === 'music' ? '乐' : '影' }}</span>
            <div>
              <strong>{{ library.Name }}</strong>
              <p>{{ library.Path }}</p>
              <p>{{ library.ChildCount || 0 }} 个条目</p>
            </div>
          </article>
        </div>
        <div v-else class="empty">
          <p>媒体库</p>
          <h2>还没有媒体库</h2>
          <p>添加媒体目录后，首页和详情页才会出现 Jellyfin 风格的内容分发逻辑。</p>
        </div>
      </div>
    </div>

    <AddLibraryDialog v-if="state.showAddLibrary" @close="state.showAddLibrary = false" />
  </section>
</template>
