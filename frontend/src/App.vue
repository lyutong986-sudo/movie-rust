<script setup lang="ts">
import { onMounted } from 'vue';
import AppLayout from './layouts/AppLayout.vue';
import LoginPage from './pages/server/LoginPage.vue';
import WizardPage from './pages/WizardPage.vue';
import { initialize, state, user } from './store/app';

onMounted(initialize);
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
    <WizardPage v-else-if="!state.startupWizardCompleted" />
    <LoginPage v-else-if="!user" />
    <AppLayout v-else />
  </main>
</template>
