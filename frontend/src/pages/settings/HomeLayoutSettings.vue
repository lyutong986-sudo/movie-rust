<script setup lang="ts">
import { ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import {
  homeSections,
  resetHomeSections,
  setHomeSections,
  type HomeSectionItem
} from '../../store/app';

const dragIndex = ref<number | null>(null);
const dragOverIndex = ref<number | null>(null);

function onDragStart(index: number, event: DragEvent) {
  dragIndex.value = index;
  event.dataTransfer!.effectAllowed = 'move';
  event.dataTransfer!.setData('text/plain', String(index));
}

function onDragOver(index: number, event: DragEvent) {
  event.preventDefault();
  event.dataTransfer!.dropEffect = 'move';
  dragOverIndex.value = index;
}

function onDragLeave() {
  dragOverIndex.value = null;
}

function onDrop(index: number, event: DragEvent) {
  event.preventDefault();
  const from = dragIndex.value;
  dragIndex.value = null;
  dragOverIndex.value = null;
  if (from === null || from === index) return;
  const list = homeSections.value.slice();
  const [item] = list.splice(from, 1);
  list.splice(index, 0, item);
  setHomeSections(list);
}

function onDragEnd() {
  dragIndex.value = null;
  dragOverIndex.value = null;
}

function toggleSection(index: number) {
  const list: HomeSectionItem[] = homeSections.value.map((s) => ({ ...s }));
  list[index].enabled = !list[index].enabled;
  setHomeSections(list);
}

const sectionIcons: Record<string, string> = {
  resume: 'i-lucide-play-circle',
  playQueue: 'i-lucide-list-video',
  watchLater: 'i-lucide-clock',
  favorites: 'i-lucide-heart',
  latest: 'i-lucide-sparkles',
  libraryLatest: 'i-lucide-library'
};
</script>

<template>
  <SettingsLayout>
    <div class="space-y-6">
      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-highlighted text-sm font-semibold">主页布局</h3>
            <UButton variant="soft" color="neutral" size="xs" icon="i-lucide-rotate-ccw" @click="resetHomeSections">
              恢复默认
            </UButton>
          </div>
        </template>

        <p class="text-muted mb-4 text-xs">拖拽调整首页各板块的显示顺序，使用开关控制是否显示。</p>

        <div class="space-y-1">
          <div
            v-for="(section, index) in homeSections"
            :key="section.id"
            class="flex items-center gap-3 rounded-lg border px-3 py-2.5 transition"
            :class="[
              dragOverIndex === index && dragIndex !== index
                ? 'border-primary bg-primary/5'
                : 'border-default bg-elevated/30 hover:bg-elevated/60',
              dragIndex === index ? 'opacity-40' : ''
            ]"
            draggable="true"
            @dragstart="onDragStart(index, $event)"
            @dragover="onDragOver(index, $event)"
            @dragleave="onDragLeave"
            @drop="onDrop(index, $event)"
            @dragend="onDragEnd"
          >
            <UIcon
              name="i-lucide-grip-vertical"
              class="size-4 shrink-0 cursor-grab text-dimmed active:cursor-grabbing"
            />
            <UIcon
              :name="sectionIcons[section.id] || 'i-lucide-layout-grid'"
              class="size-4 shrink-0"
              :class="section.enabled ? 'text-primary' : 'text-dimmed'"
            />
            <span
              class="flex-1 text-sm"
              :class="section.enabled ? 'text-highlighted' : 'text-muted line-through'"
            >
              {{ section.name }}
            </span>
            <USwitch :model-value="section.enabled" @update:model-value="toggleSection(index)" />
          </div>
        </div>
      </UCard>
    </div>
  </SettingsLayout>
</template>
