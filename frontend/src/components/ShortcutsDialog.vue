<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ 'update:open': [value: boolean] }>();

const isOpen = computed({
  get: () => props.open,
  set: (v) => emit('update:open', v)
});

const groups = [
  {
    title: '全局',
    items: [
      { keys: ['/', '⌘', 'K'], desc: '打开搜索 / 命令面板' },
      { keys: ['G', 'H'], desc: '返回首页' },
      { keys: ['G', 'S'], desc: '打开设置' },
      { keys: ['?'], desc: '快捷键列表' },
      { keys: ['Esc'], desc: '关闭当前弹窗' }
    ]
  },
  {
    title: '播放器',
    items: [
      { keys: ['Space', 'K'], desc: '播放 / 暂停' },
      { keys: ['←'], desc: '快退 10 秒（Shift+← 30 秒）' },
      { keys: ['→'], desc: '快进 10 秒（Shift+→ 30 秒）' },
      { keys: ['↑'], desc: '音量 +' },
      { keys: ['↓'], desc: '音量 -' },
      { keys: ['M'], desc: '静音' },
      { keys: ['F'], desc: '全屏' },
      { keys: ['C'], desc: '切换字幕' },
      { keys: ['N'], desc: '下一集' },
      { keys: ['P'], desc: '画中画' },
      { keys: ['0 - 9'], desc: '跳转到指定百分比' }
    ]
  }
];
</script>

<template>
  <UModal v-model:open="isOpen" :ui="{ content: 'max-w-2xl' }">
    <template #content>
      <div class="p-6">
        <div class="mb-6 flex items-center justify-between">
          <div>
            <h2 class="text-highlighted text-lg font-semibold">键盘快捷键</h2>
            <p class="text-muted text-sm">以下快捷键在本应用全局可用。</p>
          </div>
          <UButton
            color="neutral"
            variant="ghost"
            icon="i-lucide-x"
            @click="isOpen = false"
          />
        </div>
        <div class="grid gap-6 md:grid-cols-2">
          <div v-for="g in groups" :key="g.title" class="space-y-3">
            <h3 class="text-highlighted text-sm font-semibold uppercase tracking-wider">
              {{ g.title }}
            </h3>
            <ul class="space-y-1.5 text-sm">
              <li
                v-for="(item, idx) in g.items"
                :key="idx"
                class="flex items-center justify-between gap-3"
              >
                <span class="text-default">{{ item.desc }}</span>
                <span class="flex items-center gap-1">
                  <UKbd v-for="k in item.keys" :key="k">{{ k }}</UKbd>
                </span>
              </li>
            </ul>
          </div>
        </div>
      </div>
    </template>
  </UModal>
</template>
