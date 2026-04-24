<script setup lang="ts">
import SettingsNav from '../components/SettingsNav.vue';
</script>

<template>
  <div class="grid gap-6 lg:grid-cols-[220px_1fr]">
    <aside class="lg:sticky lg:top-4 lg:self-start">
      <SettingsNav />
    </aside>
    <section class="min-w-0">
      <slot />
    </section>
  </div>
</template>
