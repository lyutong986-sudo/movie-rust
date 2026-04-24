<script setup lang="ts">
withDefaults(
  defineProps<{
    icon?: string;
    title: string;
    description?: string;
    actionLabel?: string;
    actionIcon?: string;
    compact?: boolean;
  }>(),
  {
    icon: 'i-lucide-inbox',
    description: '',
    actionLabel: '',
    actionIcon: '',
    compact: false
  }
);

const emit = defineEmits<{ action: [] }>();
</script>

<template>
  <div
    class="border-default flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed text-center"
    :class="compact ? 'px-6 py-8' : 'px-8 py-14'"
  >
    <div class="bg-elevated/40 text-primary flex size-14 items-center justify-center rounded-full">
      <UIcon :name="icon" class="size-6" />
    </div>
    <div class="space-y-1">
      <h3 class="text-highlighted text-base font-semibold">{{ title }}</h3>
      <p v-if="description" class="text-muted max-w-md text-sm">{{ description }}</p>
    </div>
    <UButton
      v-if="actionLabel"
      :icon="actionIcon || undefined"
      color="primary"
      variant="soft"
      @click="emit('action')"
    >
      {{ actionLabel }}
    </UButton>
  </div>
</template>
