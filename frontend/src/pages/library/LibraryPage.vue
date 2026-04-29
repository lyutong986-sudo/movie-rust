<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../../components/MediaCard.vue';
import MediaCardSkeleton from '../../components/MediaCardSkeleton.vue';
import MediaListItem from '../../components/MediaListItem.vue';
import EmptyState from '../../components/EmptyState.vue';
import AlphaPicker from '../../components/AlphaPicker.vue';
import {
  api,
  backToParent,
  clearSelection,
  currentParentName,
  hydrateParentStack,
  items,
  libraryHasMore,
  libraryLayout,
  libraryLoadedCount,
  libraryLoadingMore,
  libraryTotalCount,
  loadMoreItems,
  loadItems,
  loadLibraryGenres,
  libraryGenresCache,
  nameStartsWith,
  parentStack,
  playAll,
  resetLibraryFilters,
  selectedItems,
  selectedLibrary,
  selectionMode,
  serializeParentStack,
  setLibraryLayout,
  shufflePlay,
  state
} from '../../store/app';
import type { BaseItemDto } from '../../api/emby';
import { itemRoute, playbackRoute } from '../../utils/navigation';
import { useAppToast } from '../../composables/toast';

const toast = useAppToast();

const route = useRoute();
const router = useRouter();
const loadMoreTrigger = ref<HTMLElement | null>(null);
let loadMoreObserver: IntersectionObserver | null = null;

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
  { value: 'CommunityRating', label: '评分' },
  { value: 'Random', label: '随机' },
  { value: 'IndexNumber', label: '集数' }
];

const yearOptions = computed(() => {
  const now = new Date().getFullYear();
  const list: Array<{ label: string; value: number }> = [];
  for (let y = now; y >= 1960; y -= 1) list.push({ label: String(y), value: y });
  return list;
});

const genreOptions = computed(() => {
  const libraryId = state.selectedLibraryId;
  if (!libraryId) return [] as Array<{ label: string; value: string }>;
  return (libraryGenresCache.value[libraryId] || []).map((name) => ({ label: name, value: name }));
});

const filterBadgeCount = computed(() => {
  let count = 0;
  count += state.libraryGenres.length;
  count += state.libraryYears.length;
  if (state.libraryFavoritesOnly) count += 1;
  if (state.libraryOnly4K) count += 1;
  if (state.libraryOnlyHDR) count += 1;
  if (state.librarySubtitlesOnly) count += 1;
  return count;
});

// 路由 → parent 栈的同步
// 加入 `?path=a,b,c` 记录 parent 链，刷新时恢复面包屑。
watch(
  () => route.params.id,
  async (value) => {
    if (typeof value !== 'string' || !value) return;
    const switched = state.selectedLibraryId !== value;
    if (switched) {
      // 只更新 store 状态，不在这里 loadItems —— 后面统一拉一次。
      state.selectedLibraryId = value;
      state.search = '';
      parentStack.value = [];
    }
    const pathParam = typeof route.query.path === 'string' ? route.query.path : '';
    await hydrateParentStack(pathParam);
    await loadItems();
    await loadLibraryGenres(value);
  },
  { immediate: true }
);

watch(
  () => [
    state.libraryViewType,
    state.librarySortBy,
    state.librarySortAscending,
    state.libraryGenres.slice(),
    state.libraryYears.slice(),
    state.libraryFavoritesOnly,
    state.libraryOnly4K,
    state.libraryOnlyHDR,
    state.librarySubtitlesOnly,
    nameStartsWith.value
  ],
  async () => {
    if (state.selectedLibraryId) {
      await loadItems();
    }
  },
  { deep: true }
);

// parentStack 变化时同步到 URL
watch(
  () => parentStack.value.length,
  () => {
    if (!state.selectedLibraryId) return;
    const path = serializeParentStack();
    const nextQuery = path ? { ...route.query, path } : { ...route.query, path: undefined };
    void router.replace({ path: route.path, query: nextQuery });
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

function resetLoadMoreObserver() {
  loadMoreObserver?.disconnect();
  loadMoreObserver = null;
  if (!loadMoreTrigger.value || !libraryHasMore.value) {
    return;
  }
  loadMoreObserver = new IntersectionObserver(
    (entries) => {
      const shouldLoad = entries.some((entry) => entry.isIntersecting);
      if (shouldLoad) {
        void loadMoreItems();
      }
    },
    { rootMargin: '480px 0px' }
  );
  loadMoreObserver.observe(loadMoreTrigger.value);
}

watch(
  () => [items.value.length, libraryHasMore.value, libraryLoadingMore.value],
  () => {
    void nextTick(resetLoadMoreObserver);
  }
);

watch(loadMoreTrigger, () => {
  void nextTick(resetLoadMoreObserver);
});

onMounted(() => {
  void nextTick(resetLoadMoreObserver);
});

onBeforeUnmount(() => {
  loadMoreObserver?.disconnect();
});

const selectedCount = computed(() => selectedItems.size);
const batchBusy = ref(false);

async function batchMarkPlayed() {
  batchBusy.value = true;
  try {
    for (const id of selectedItems) {
      await api.markPlayed(id, true);
    }
    toast.success(`已将 ${selectedItems.size} 项标记为已播放`);
    clearSelection();
    await loadItems();
  } catch (err: any) {
    toast.error('批量操作失败: ' + (err?.message || err));
  } finally {
    batchBusy.value = false;
  }
}

async function batchMarkFavorite() {
  batchBusy.value = true;
  try {
    for (const id of selectedItems) {
      await api.markFavorite(id, true);
    }
    toast.success(`已将 ${selectedItems.size} 项标记为收藏`);
    clearSelection();
    await loadItems();
  } catch (err: any) {
    toast.error('批量操作失败: ' + (err?.message || err));
  } finally {
    batchBusy.value = false;
  }
}

async function batchRefreshMetadata() {
  batchBusy.value = true;
  try {
    for (const id of selectedItems) {
      await api.refreshItemMetadata(id, {
        metadataRefreshMode: 'FullRefresh',
        imageRefreshMode: 'FullRefresh',
        replaceAllMetadata: false,
        replaceAllImages: false
      });
    }
    toast.success(`已提交 ${selectedItems.size} 项的元数据刷新`);
    clearSelection();
  } catch (err: any) {
    toast.error('批量操作失败: ' + (err?.message || err));
  } finally {
    batchBusy.value = false;
  }
}

async function batchDelete() {
  if (!window.confirm(`确定要删除选中的 ${selectedItems.size} 项吗？此操作不可撤销。`)) return;
  batchBusy.value = true;
  try {
    for (const id of selectedItems) {
      await api.deleteItem(id);
    }
    toast.success(`已删除 ${selectedItems.size} 项`);
    clearSelection();
    await loadItems();
  } catch (err: any) {
    toast.error('批量删除失败: ' + (err?.message || err));
  } finally {
    batchBusy.value = false;
  }
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
        <UIcon v-if="index" name="i-lucide-chevron-right" class="text-muted size-3" />
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
      class="border-default bg-elevated/20 flex flex-col gap-3 rounded-xl border p-3 sm:flex-row sm:items-center sm:justify-between"
    >
      <div class="min-w-0">
        <h2 class="text-highlighted truncate text-base font-semibold">{{ currentParentName }}</h2>
        <p class="text-muted text-xs">
          {{ libraryLoadedCount }} / {{ libraryTotalCount }} 个条目
        </p>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <USelect v-model="state.libraryViewType" :items="VIEW_TYPES" size="sm" class="w-32" />

        <UPopover>
          <UButton
            color="neutral"
            variant="soft"
            size="sm"
            icon="i-lucide-sliders-horizontal"
            :badge="filterBadgeCount || undefined"
          >
            筛选
            <UBadge
              v-if="filterBadgeCount"
              color="primary"
              variant="solid"
              size="xs"
              class="ms-1"
            >
              {{ filterBadgeCount }}
            </UBadge>
          </UButton>
          <template #content>
            <div class="w-80 space-y-3 p-3">
              <div>
                <p class="text-muted mb-1 text-xs font-semibold uppercase tracking-wider">
                  类型
                </p>
                <USelectMenu
                  v-model="state.libraryGenres"
                  :items="genreOptions"
                  value-key="value"
                  multiple
                  placeholder="选择类型"
                  class="w-full"
                  size="sm"
                />
              </div>
              <div>
                <p class="text-muted mb-1 text-xs font-semibold uppercase tracking-wider">
                  年份
                </p>
                <USelectMenu
                  v-model="state.libraryYears"
                  :items="yearOptions"
                  value-key="value"
                  multiple
                  placeholder="选择年份"
                  class="w-full"
                  size="sm"
                />
              </div>
              <USeparator />
              <div class="space-y-2">
                <label class="flex items-center gap-2 text-sm">
                  <UCheckbox v-model="state.libraryFavoritesOnly" />
                  只显示收藏
                </label>
                <label class="flex items-center gap-2 text-sm">
                  <UCheckbox v-model="state.libraryOnly4K" />
                  仅 4K 分辨率
                </label>
                <label class="flex items-center gap-2 text-sm">
                  <UCheckbox v-model="state.libraryOnlyHDR" />
                  仅 HDR / Dolby Vision
                </label>
                <label class="flex items-center gap-2 text-sm">
                  <UCheckbox v-model="state.librarySubtitlesOnly" />
                  有字幕
                </label>
              </div>
              <div class="flex justify-between pt-1">
                <UButton size="xs" color="neutral" variant="ghost" @click="resetLibraryFilters">
                  重置
                </UButton>
                <span class="text-muted text-xs">{{ filterBadgeCount }} 个过滤器</span>
              </div>
            </div>
          </template>
        </UPopover>

        <UButton
          color="neutral"
          variant="soft"
          size="sm"
          icon="i-lucide-play"
          :disabled="!items.length"
          @click="playAll(items)"
        >
          全部播放
        </UButton>
        <UButton
          color="neutral"
          variant="soft"
          size="sm"
          icon="i-lucide-shuffle"
          :disabled="!items.length"
          @click="shufflePlay(items)"
        >
          随机播放
        </UButton>

        <USelect v-model="state.librarySortBy" :items="SORT_OPTIONS" size="sm" class="w-36" />
        <UButton
          color="neutral"
          variant="soft"
          size="sm"
          :icon="
            state.librarySortAscending ? 'i-lucide-arrow-up-narrow-wide' : 'i-lucide-arrow-down-wide-narrow'
          "
          @click="toggleSortOrder"
        >
          {{ state.librarySortAscending ? '升序' : '降序' }}
        </UButton>

        <div class="border-default flex items-center gap-0.5 rounded-lg border p-0.5">
          <UButton
            color="neutral"
            :variant="libraryLayout === 'grid' ? 'solid' : 'ghost'"
            size="xs"
            icon="i-lucide-grid-3x3"
            title="网格"
            @click="setLibraryLayout('grid')"
          />
          <UButton
            color="neutral"
            :variant="libraryLayout === 'list' ? 'solid' : 'ghost'"
            size="xs"
            icon="i-lucide-list"
            title="列表"
            @click="setLibraryLayout('list')"
          />
          <UButton
            color="neutral"
            :variant="libraryLayout === 'detail' ? 'solid' : 'ghost'"
            size="xs"
            icon="i-lucide-layout-list"
            title="详情"
            @click="setLibraryLayout('detail')"
          />
        </div>
      </div>
    </div>

    <AlphaPicker v-model="nameStartsWith" />

    <Transition
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="translate-y-4 opacity-0"
      enter-to-class="translate-y-0 opacity-100"
      leave-active-class="transition duration-150 ease-in"
      leave-from-class="translate-y-0 opacity-100"
      leave-to-class="translate-y-4 opacity-0"
    >
      <div
        v-if="selectedCount > 0"
        class="bg-elevated border-default sticky top-0 z-30 flex flex-wrap items-center justify-between gap-2 rounded-xl border p-3 shadow-lg"
      >
        <span class="text-highlighted text-sm font-medium">已选择 {{ selectedCount }} 项</span>
        <div class="flex flex-wrap items-center gap-2">
          <UButton color="primary" variant="soft" size="sm" icon="i-lucide-check" :loading="batchBusy" @click="batchMarkPlayed">
            标记已播放
          </UButton>
          <UButton color="primary" variant="soft" size="sm" icon="i-lucide-heart" :loading="batchBusy" @click="batchMarkFavorite">
            标记收藏
          </UButton>
          <UButton color="primary" variant="soft" size="sm" icon="i-lucide-refresh-cw" :loading="batchBusy" @click="batchRefreshMetadata">
            刷新元数据
          </UButton>
          <UButton color="error" variant="soft" size="sm" icon="i-lucide-trash-2" :loading="batchBusy" @click="batchDelete">
            删除
          </UButton>
          <UButton color="neutral" variant="ghost" size="sm" icon="i-lucide-x" @click="clearSelection">
            取消选择
          </UButton>
        </div>
      </div>
    </Transition>

    <div
      v-if="state.busy && !items.length"
      class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 xl:grid-cols-7"
    >
      <MediaCardSkeleton v-for="i in 14" :key="i" />
    </div>
    <template v-else-if="items.length">
      <div
        v-if="libraryLayout === 'grid'"
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
      <div v-else class="flex flex-col gap-1">
        <MediaListItem
          v-for="item in items"
          :key="item.Id"
          :item="item"
          :detailed="libraryLayout === 'detail'"
          @select="openMedia"
        />
      </div>
    </template>
    <div v-if="items.length" class="flex flex-col items-center gap-2 py-2">
      <div ref="loadMoreTrigger" class="h-0.5 w-full" />
      <UButton
        v-if="libraryHasMore"
        :loading="libraryLoadingMore"
        color="neutral"
        variant="soft"
        size="sm"
        @click="loadMoreItems"
      >
        加载更多
      </UButton>
      <span v-else class="text-muted text-xs">已加载全部条目</span>
    </div>
    <EmptyState
      v-else
      icon="i-lucide-inbox"
      title="这里暂时没有内容"
      description="可以尝试切换排序、清空筛选，或者先在管理员页面执行一次媒体扫描。"
      :action-label="filterBadgeCount ? '清空筛选' : ''"
      action-icon="i-lucide-sliders-horizontal"
      @action="resetLibraryFilters"
    />
  </div>
</template>
