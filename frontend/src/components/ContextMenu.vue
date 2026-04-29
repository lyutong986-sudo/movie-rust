<script setup lang="ts">
import { ref, watch, nextTick, onBeforeUnmount } from 'vue';

export interface ContextMenuItem {
  label: string;
  icon?: string;
  disabled?: boolean;
  color?: string;
  onSelect?: () => void;
}

const props = defineProps<{
  items: ContextMenuItem[][];
  previewImage?: string;
  previewTitle?: string;
  previewSubtitle?: string;
}>();

const open = ref(false);
const menuX = ref(0);
const menuY = ref(0);
const menuEl = ref<HTMLElement | null>(null);

function show(e: MouseEvent) {
  e.preventDefault();
  e.stopPropagation();
  menuX.value = e.clientX;
  menuY.value = e.clientY;
  open.value = true;
  nextTick(clampPosition);
}

function hide() {
  open.value = false;
}

function clampPosition() {
  if (!menuEl.value) return;
  const rect = menuEl.value.getBoundingClientRect();
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  if (menuX.value + rect.width > vw - 8) menuX.value = vw - rect.width - 8;
  if (menuY.value + rect.height > vh - 8) menuY.value = vh - rect.height - 8;
  if (menuX.value < 8) menuX.value = 8;
  if (menuY.value < 8) menuY.value = 8;
}

function onAction(item: ContextMenuItem) {
  if (item.disabled) return;
  hide();
  item.onSelect?.();
}

function onClickOutside(e: MouseEvent) {
  if (menuEl.value && !menuEl.value.contains(e.target as Node)) hide();
}

watch(open, (v) => {
  if (v) {
    setTimeout(() => document.addEventListener('mousedown', onClickOutside), 0);
  } else {
    document.removeEventListener('mousedown', onClickOutside);
  }
});

onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onClickOutside);
});

defineExpose({ show, hide });
</script>

<template>
  <Teleport to="body">
    <Transition name="ctx">
      <div
        v-if="open"
        ref="menuEl"
        class="context-menu fixed z-[9999] min-w-[200px] max-w-[320px] overflow-hidden rounded-lg border border-default bg-elevated shadow-xl"
        :style="{ left: `${menuX}px`, top: `${menuY}px` }"
        @contextmenu.prevent
      >
        <!-- Preview header -->
        <div
          v-if="previewImage || previewTitle"
          class="flex items-center gap-3 border-b border-default px-3 py-2.5"
        >
          <div
            v-if="previewImage"
            class="h-12 w-8 shrink-0 overflow-hidden rounded bg-default bg-cover bg-center"
            :style="{ backgroundImage: `url(${previewImage})` }"
          />
          <div v-if="previewTitle" class="min-w-0 flex-1">
            <p class="truncate text-sm font-medium text-highlighted">{{ previewTitle }}</p>
            <p v-if="previewSubtitle" class="truncate text-xs text-muted">{{ previewSubtitle }}</p>
          </div>
        </div>

        <div class="max-h-[60vh] overflow-y-auto py-1">
          <template v-for="(group, gi) in items" :key="gi">
            <div v-if="gi > 0" class="my-1 border-t border-default" />
            <button
              v-for="item in group"
              :key="item.label"
              type="button"
              class="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-sm transition-colors"
              :class="[
                item.disabled
                  ? 'cursor-not-allowed opacity-40'
                  : 'cursor-pointer hover:bg-default/80',
                item.color === 'error' ? 'text-error hover:bg-error/10' : 'text-highlighted'
              ]"
              :disabled="item.disabled"
              @click="onAction(item)"
            >
              <UIcon v-if="item.icon" :name="item.icon" class="size-4 shrink-0 text-muted" />
              <span class="truncate">{{ item.label }}</span>
            </button>
          </template>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.ctx-enter-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.ctx-leave-active {
  transition: opacity 0.08s ease;
}
.ctx-enter-from {
  opacity: 0;
  transform: scale(0.95);
}
.ctx-leave-to {
  opacity: 0;
}
</style>
