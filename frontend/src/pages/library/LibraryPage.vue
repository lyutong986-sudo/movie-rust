<script setup lang="ts">
import { computed, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../../components/MediaCard.vue';
import MediaCardSkeleton from '../../components/MediaCardSkeleton.vue';
import {
  backToParent,
  currentParentName,
  items,
  loadItems,
  parentStack,
  selectedLibrary,
  selectLibrary,
  state
} from '../../store/app';
import type { BaseItemDto } from '../../api/emby';
import { itemRoute, playbackRoute } from '../../utils/navigation';

const route = useRoute();
const router = useRouter();

const VIEW_TYPES = [
  { value: '', label: '全部' },
  { value: 'Movie', label: '电影' },
  { value: 'Series', label: '剧集' },
  { value: 'Season', label: '季' },
  { value: 'Episode', label: '单集' },
  { value: 'Folder', label: '文件夹' }
];

const SORT_OPTIONS = [
  { value: 'SortName', label: '名称' },
  { value: 'DateCreated', label: '添加时间' },
  { value: 'ProductionYear', label: '年份' },
  { value: 'IndexNumber', label: '集数' }
];

watch(
  () => route.params.id,
  async (value) => {
    if (typeof value === 'string' && value) {
      await selectLibrary(value);
    }
  },
  { immediate: true }
);

watch(
  () => [state.libraryViewType, state.librarySortBy, state.librarySortAscending],
  async () => {
    if (state.selectedLibraryId) {
      await loadItems();
    }
  }
);

const breadcrumbs = computed(() =>
  [selectedLibrary.value?.Name, ...parentStack.value.map((item) => item.Name)].filter(Boolean)
);

async function openMedia(item: BaseItemDto) {
  if (item.IsFolder && item.Type !== 'Series') {
    if (item.Type === 'CollectionFolder') {
      await router.push(`/library/${item.Id}`);
      return;
    }

    parentStack.value.push(item);
    await loadItems();
    return;
  }

  await router.push(itemRoute(item));
}

async function playItem(item: BaseItemDto) {
  await router.push(playbackRoute(item));
}

function toggleSortOrder() {
  state.librarySortAscending = !state.librarySortAscending;
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <nav class="flex flex-wrap items-center gap-2 text-sm">
      <UButton
        v-if="parentStack.length"
        color="neutral"
        variant="ghost"
        size="xs"
        icon="i-lucide-arrow-left"
        @click="backToParent"
      >
        返回
      </UButton>
      <template v-for="(crumb, index) in breadcrumbs" :key="crumb">
        <UIcon v-if="index" name="i-lucide-chevron-right" class="size-3 text-muted" />
        <span
          :class="
            index === breadcrumbs.length - 1 ? 'text-highlighted font-medium' : 'text-muted'
          "
        >
          {{ crumb }}
        </span>
      </template>
    </nav>

    <div
      class="flex flex-col gap-3 rounded-xl border border-default bg-elevated/20 p-3 sm:flex-row sm:items-center sm:justify-between"
    >
      <div>
        <h2 class="text-highlighted text-base font-semibold">{{ currentParentName }}</h2>
        <p class="text-muted text-xs">{{ items.length }} 个条目</p>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <USelect v-model="state.libraryViewType" :items="VIEW_TYPES" size="sm" class="w-32" />
        <USelect v-model="state.librarySortBy" :items="SORT_OPTIONS" size="sm" class="w-36" />
        <UButton
          color="neutral"
          variant="soft"
          size="sm"
          :icon="state.librarySortAscending ? 'i-lucide-arrow-up-narrow-wide' : 'i-lucide-arrow-down-wide-narrow'"
          @click="toggleSortOrder"
        >
          {{ state.librarySortAscending ? '升序' : '降序' }}
        </UButton>
      </div>
    </div>

    <div
      v-if="state.busy && !items.length"
      class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 xl:grid-cols-7"
    >
      <MediaCardSkeleton v-for="i in 14" :key="i" />
    </div>
    <div
      v-else-if="items.length"
      class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 xl:grid-cols-7"
    >
      <MediaCard
        v-for="item in items"
        :key="item.Id"
        :item="item"
        @play="playItem"
        @select="openMedia"
      />
    </div>
    <div
      v-else
      class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default bg-elevated/20 p-10 text-center"
    >
      <UIcon name="i-lucide-inbox" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">这里暂时没有内容</h3>
      <p class="text-muted max-w-md text-sm">
        可以尝试切换排序、清空筛选，或者先在管理员页面执行一次媒体扫描。
      </p>
    </div>
  </div>
</template>
