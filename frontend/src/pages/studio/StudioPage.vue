<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../../components/MediaCard.vue';
import type { BaseItemDto } from '../../api/emby';
import { api } from '../../store/app';
import { itemRoute, playbackRoute } from '../../utils/navigation';

const route = useRoute();
const router = useRouter();

const loading = ref(true);
const error = ref('');
const studio = ref<BaseItemDto | null>(null);
const items = ref<BaseItemDto[]>([]);

const studioName = computed(() => {
  const raw = String(route.params.id || '');
  try {
    return decodeURIComponent(raw);
  } catch {
    return raw;
  }
});

const studioImageUrl = computed(() => {
  if (!studio.value) return '';
  return api.itemImageUrl(studio.value) || api.thumbUrl(studio.value);
});

watch(
  () => route.params.id,
  async () => {
    if (!studioName.value) return;
    await loadStudio();
  },
  { immediate: true }
);

async function loadStudio() {
  loading.value = true;
  error.value = '';
  studio.value = null;
  items.value = [];

  try {
    const [studioData, studioItems] = await Promise.all([
      api.getStudio(studioName.value),
      api.getStudioItems(studioName.value, { limit: 240 })
    ]);
    studio.value = studioData;
    items.value = studioItems;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

async function openItem(item: BaseItemDto) {
  await router.push(itemRoute(item));
}

async function playItem(item: BaseItemDto) {
  await router.push(playbackRoute(item));
}
</script>

<template>
  <div class="flex flex-col gap-6">
    <nav class="flex items-center gap-2 text-sm">
      <UButton
        color="neutral"
        variant="ghost"
        size="xs"
        icon="i-lucide-arrow-left"
        @click="router.back()"
      >
        返回
      </UButton>
      <UIcon name="i-lucide-chevron-right" class="size-3 text-muted" />
      <span class="text-muted">制片工作室</span>
      <UIcon name="i-lucide-chevron-right" class="size-3 text-muted" />
      <span class="text-highlighted font-medium">{{ studioName || '加载中…' }}</span>
    </nav>

    <div v-if="loading" class="flex min-h-[40vh] flex-col items-center justify-center gap-2">
      <UProgress animation="carousel" class="w-48" />
      <p class="text-muted text-sm">正在加载工作室信息…</p>
    </div>

    <UAlert
      v-else-if="error"
      color="error"
      variant="subtle"
      icon="i-lucide-triangle-alert"
      title="加载失败"
      :description="error"
    />

    <template v-else-if="studio">
      <div class="flex items-center gap-4">
        <div class="flex h-14 w-14 items-center justify-center rounded-xl bg-primary/10 text-primary">
          <img
            v-if="studioImageUrl"
            :src="studioImageUrl"
            :alt="studio.Name"
            class="h-full w-full rounded-xl object-contain"
          />
          <UIcon v-else name="i-lucide-building-2" class="size-7" />
        </div>
        <div>
          <h1 class="text-highlighted text-2xl font-bold">{{ studio.Name }}</h1>
          <p class="text-muted text-sm">{{ items.length }} 部作品</p>
        </div>
      </div>

      <p
        v-if="studio.Overview"
        class="text-dimmed max-w-3xl text-sm leading-relaxed"
      >
        {{ studio.Overview }}
      </p>

      <section v-if="items.length" class="space-y-4">
        <div class="flex items-center gap-2">
          <UIcon name="i-lucide-clapperboard" class="size-5 text-primary" />
          <h2 class="text-highlighted text-lg font-semibold">作品列表</h2>
          <span class="text-muted text-sm">({{ items.length }})</span>
        </div>
        <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 xl:grid-cols-7">
          <MediaCard
            v-for="item in items"
            :key="item.Id"
            :item="item"
            @play="playItem"
            @select="openItem"
          />
        </div>
      </section>

      <div
        v-else
        class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default bg-elevated/20 p-10 text-center"
      >
        <UIcon name="i-lucide-building-2" class="size-10 text-muted" />
        <h3 class="text-highlighted text-lg font-semibold">暂无相关作品</h3>
        <p class="text-muted max-w-md text-sm">
          当前媒体库中没有找到此工作室的相关作品。
        </p>
      </div>
    </template>
  </div>
</template>
