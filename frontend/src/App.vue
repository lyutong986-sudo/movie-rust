<script setup lang="ts">
import { computed, onMounted, watch } from 'vue';
import { RouterView, useRoute, useRouter } from 'vue-router';
import AppLayout from './layouts/AppLayout.vue';
import AuthLayout from './layouts/AuthLayout.vue';
import { initialize, state, user } from './store/app';

const route = useRoute();
const router = useRouter();

const layout = computed(() => (route.meta.layout as string | undefined) ?? 'app');

onMounted(initialize);

watch(
  () => [state.initialized, state.startupWizardCompleted, user.value, route.fullPath] as const,
  async () => {
    if (!state.initialized) {
      return;
    }

    if (!state.startupWizardCompleted) {
      if (route.name !== 'wizard') {
        await router.replace('/wizard');
      }
      return;
    }

    if (!user.value) {
      if (layout.value !== 'server') {
        await router.replace('/server/login');
      }
      return;
    }

    if (layout.value === 'server') {
      await router.replace('/');
    }
  },
  { immediate: true }
);
</script>

<template>
  <UApp>
    <!-- 首次加载：简洁 loading 屏 -->
    <div
      v-if="!state.initialized"
      class="flex min-h-screen items-center justify-center bg-(--ui-bg)"
    >
      <div class="flex flex-col items-center gap-4">
        <div class="flex h-14 w-14 items-center justify-center rounded-xl bg-primary text-primary-contrast text-lg font-bold">
          MR
        </div>
        <div class="text-highlighted text-base font-medium">{{ state.serverName }}</div>
        <UProgress animation="carousel" class="w-48" />
        <p class="text-muted text-sm">正在连接服务器……</p>
      </div>
    </div>

    <!-- 登录 / 选服 / 添加服务器 / 引导 -->
    <AuthLayout v-else-if="layout === 'server'">
      <RouterView />
    </AuthLayout>

    <!-- 全屏播放等：不套 shell -->
    <RouterView v-else-if="layout === 'fullpage'" />

    <!-- 主应用：Dashboard 布局 -->
    <AppLayout v-else>
      <RouterView />
    </AppLayout>
  </UApp>
</template>
