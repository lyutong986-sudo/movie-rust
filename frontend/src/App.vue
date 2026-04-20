<script setup lang="ts">
import { onMounted, watch } from 'vue';
import { RouterView, useRoute, useRouter } from 'vue-router';
import AppLayout from './layouts/AppLayout.vue';
import { initialize, state, user } from './store/app';

const route = useRoute();
const router = useRouter();

onMounted(initialize);

watch(
  () => [state.initialized, state.startupWizardCompleted, user.value, route.fullPath],
  async () => {
    if (!state.initialized) {
      return;
    }

    const isServerRoute = Boolean(route.meta.layout === 'server');

    if (!state.startupWizardCompleted) {
      if (route.name !== 'wizard') {
        await router.replace('/wizard');
      }
      return;
    }

    if (!user.value) {
      if (!isServerRoute) {
        await router.replace('/server/login');
      }
      return;
    }

    if (isServerRoute) {
      await router.replace('/');
    }
  },
  { immediate: true }
);
</script>

<template>
  <main class="app-shell">
    <section v-if="!state.initialized" class="server-screen">
      <div class="server-card">
        <div class="server-brand centered">
          <div class="mark">MR</div>
          <h1>{{ state.serverName }}</h1>
          <p>正在连接服务器</p>
        </div>
      </div>
    </section>
    <RouterView v-else-if="!state.startupWizardCompleted || !user || route.meta.layout === 'server'" />
    <AppLayout v-else />
  </main>
</template>
