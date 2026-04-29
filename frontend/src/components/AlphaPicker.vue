<script setup lang="ts">
const LETTERS = ['#', ...'ABCDEFGHIJKLMNOPQRSTUVWXYZ'];

const props = defineProps<{
  modelValue: string;
}>();

const emit = defineEmits<{
  'update:modelValue': [value: string];
}>();

function pick(letter: string) {
  emit('update:modelValue', props.modelValue === letter ? '' : letter);
}
</script>

<template>
  <div class="alpha-picker flex flex-wrap items-center gap-0.5 rounded-lg bg-elevated/40 px-2 py-1.5">
    <button
      v-for="letter in LETTERS"
      :key="letter"
      type="button"
      class="inline-flex h-7 w-7 items-center justify-center rounded text-xs font-medium transition-colors hover:bg-elevated hover:text-highlighted"
      :class="modelValue === letter ? 'bg-primary text-primary-contrast font-bold' : 'text-muted'"
      @click="pick(letter)"
    >
      {{ letter }}
    </button>
  </div>
</template>
