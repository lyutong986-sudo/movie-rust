<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { api, libraries } from '../store/app';
import type { BaseItemDto } from '../api/emby';
import { itemRoute, playbackRoute } from '../utils/navigation';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ 'update:open': [value: boolean] }>();

const router = useRouter();
const query = ref('');
const searching = ref(false);
const results = ref<BaseItemDto[]>([]);

let timer = 0;

const isOpen = computed({
  get: () => props.open,
  set: (v) => emit('update:open', v)
});

watch(
  () => props.open,
  (v) => {
    if (v) {
      query.value = '';
      results.value = [];
    }
  }
);

watch(query, (value) => {
  window.clearTimeout(timer);
  timer = window.setTimeout(async () => {
    const q = value.trim();
    if (!q) {
      results.value = [];
      return;
    }
    searching.value = true;
    try {
      const res = await api.items(undefined, q, true, { limit: 20 });
      results.value = res.Items || [];
    } catch {
      results.value = [];
    } finally {
      searching.value = false;
    }
  }, 250);
});

const navGroups = computed(() => [
  {
    id: 'navigation',
    label: '导航',
    items: [
      { label: '首页', icon: 'i-lucide-home', to: '/' },
      { label: '搜索', icon: 'i-lucide-search', to: '/search' },
      { label: '设置', icon: 'i-lucide-settings', to: '/settings' },
      { label: '账户', icon: 'i-lucide-user', to: '/settings/account' },
      { label: '媒体库管理', icon: 'i-lucide-folder-cog', to: '/settings/libraries' }
    ]
  },
  {
    id: 'libraries',
    label: '媒体库',
    items: libraries.value.map((lib) => ({
      label: lib.Name,
      icon: 'i-lucide-folder',
      to: `/library/${lib.Id}`
    }))
  }
]);

async function goTo(to: string) {
  emit('update:open', false);
  await router.push(to);
}

async function openItem(item: BaseItemDto) {
  emit('update:open', false);
  if (item.IsFolder && item.Type !== 'Series' && item.Type !== 'Season') {
    await router.push(`/library/${item.Id}`);
  } else {
    await router.push(itemRoute(item));
  }
}

async function playItem(item: BaseItemDto) {
  emit('update:open', false);
  await router.push(playbackRoute(item));
}
</script>

<template>
  <UModal v-model:open="isOpen" :ui="{ content: 'max-w-2xl' }">
    <template #content>
      <div class="flex flex-col">
        <div class="border-default flex items-center gap-2 border-b p-3">
          <UIcon name="i-lucide-search" class="text-muted size-5" />
          <input
            v-model="query"
            autofocus
            placeholder="搜索内容、跳转页面、执行操作…"
            class="w-full bg-transparent text-base outline-none"
          />
          <span class="text-muted hidden items-center gap-1 text-xs sm:flex">
            <UKbd>Esc</UKbd> 关闭
          </span>
        </div>

        <div class="max-h-[60vh] overflow-y-auto p-2">
          <!-- 搜索结果 -->
          <div v-if="query.trim()" class="space-y-1">
            <p class="text-muted px-2 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-wider">
              内容
            </p>
            <div v-if="searching" class="text-muted p-3 text-sm">搜索中…</div>
            <div v-else-if="!results.length" class="text-muted p-3 text-sm">没有匹配结果</div>
            <button
              v-for="r in results"
              :key="r.Id"
              type="button"
              class="hover:bg-elevated flex w-full items-center gap-3 rounded px-2 py-2 text-left"
              @click="openItem(r)"
            >
              <div class="bg-elevated h-10 w-7 shrink-0 overflow-hidden rounded">
                <img
                  v-if="api.itemImageUrl(r)"
                  :src="api.itemImageUrl(r)"
                  :alt="r.Name"
                  class="h-full w-full object-cover"
                />
              </div>
              <div class="min-w-0 flex-1">
                <p class="text-highlighted truncate text-sm font-medium">{{ r.Name }}</p>
                <p class="text-muted truncate text-xs">
                  {{ [r.Type, r.ProductionYear].filter(Boolean).join(' · ') }}
                </p>
              </div>
              <UButton
                v-if="!r.IsFolder && r.MediaSources?.length"
                size="xs"
                color="primary"
                variant="soft"
                icon="i-lucide-play"
                @click.stop="playItem(r)"
              />
            </button>
          </div>

          <!-- 导航分组 -->
          <div v-else class="space-y-2">
            <div v-for="group in navGroups" :key="group.id" class="space-y-0.5">
              <p class="text-muted px-2 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-wider">
                {{ group.label }}
              </p>
              <button
                v-for="item in group.items"
                :key="item.to"
                type="button"
                class="hover:bg-elevated flex w-full items-center gap-3 rounded px-2 py-2 text-left text-sm"
                @click="goTo(item.to)"
              >
                <UIcon :name="item.icon" class="text-muted size-4" />
                <span class="text-highlighted">{{ item.label }}</span>
              </button>
            </div>
          </div>
        </div>

        <div class="border-default text-muted flex items-center justify-between border-t px-4 py-2 text-xs">
          <span><UKbd>↑</UKbd> <UKbd>↓</UKbd> 浏览</span>
          <span><UKbd>Enter</UKbd> 打开</span>
          <span><UKbd>?</UKbd> 快捷键</span>
        </div>
      </div>
    </template>
  </UModal>
</template>
