<script setup lang="ts">
import { state } from '../store/app';
</script>

<template>
  <div class="relative min-h-screen overflow-hidden bg-(--ui-bg)">
    <div
      class="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_20%_20%,rgba(56,189,248,0.15),transparent_55%),radial-gradient(circle_at_80%_80%,rgba(99,102,241,0.12),transparent_55%)]"
    />

    <div class="relative z-10 flex min-h-screen items-center justify-center p-4 sm:p-6 lg:p-8">
      <div class="w-full max-w-xl space-y-6">
        <header class="flex items-center gap-3">
          <div class="flex h-10 w-10 items-center justify-center rounded-xl bg-primary text-primary-contrast text-sm font-bold">
            MR
          </div>
          <div>
            <p class="text-muted text-xs uppercase tracking-wider">Movie Rust</p>
            <h1 class="text-highlighted text-lg font-semibold">{{ state.serverName }}</h1>
          </div>
        </header>

        <UCard :ui="{ body: 'p-6 sm:p-8' }">
          <slot />
        </UCard>

        <p v-if="state.error" class="text-error text-sm">{{ state.error }}</p>
        <p v-else-if="state.message" class="text-muted text-sm">{{ state.message }}</p>
      </div>
    </div>
  </div>
</template>
