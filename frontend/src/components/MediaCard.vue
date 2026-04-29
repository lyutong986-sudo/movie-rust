<script setup lang="ts">
import { computed, ref, watch, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import type { BaseItemDto } from '../api/emby';
import {
  api,
  enqueue,
  enableSelectionMode,
  isAdmin,
  isInWatchLater,
  itemSubtitle,
  selectedItems,
  selectionMode,
  toggleFavorite,
  togglePlayed,
  toggleSelection,
  toggleWatchLater
} from '../store/app';
import { itemRoute } from '../utils/navigation';
import { useAppToast } from '../composables/toast';
import MediaQualityBadges from './MediaQualityBadges.vue';
import ContextMenu from './ContextMenu.vue';
import type { ContextMenuItem } from './ContextMenu.vue';
import CollectionEditorDialog from './CollectionEditorDialog.vue';

const props = defineProps<{
  item: BaseItemDto;
  subtitle?: string;
  thumb?: boolean;
}>();

const emit = defineEmits<{
  play: [item: BaseItemDto];
  select: [item: BaseItemDto];
  deleted: [item: BaseItemDto];
  editMetadata: [item: BaseItemDto];
  identify: [item: BaseItemDto];
}>();

const router = useRouter();
const toast = useAppToast();
const imageError = ref(false);
const blurhashUrl = ref('');
const ctxMenu = ref<InstanceType<typeof ContextMenu> | null>(null);
const collectionDialogOpen = ref(false);
const collectionDialogItemIds = ref<string[]>([]);

function extractBlurhash(item: BaseItemDto): string | null {
  const hashes = item.ImageBlurHashes;
  if (!hashes) return null;
  for (const type of ['Primary', 'Backdrop', 'Thumb']) {
    const bucket = hashes[type];
    if (bucket) {
      const first = Object.values(bucket)[0];
      if (first) return first;
    }
  }
  return null;
}

async function decodeBlurhash(hash: string): Promise<string> {
  const { decode } = await import('blurhash');
  const w = 32, h = 32;
  const pixels = decode(hash, w, h);
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext('2d')!;
  const imageData = ctx.createImageData(w, h);
  imageData.data.set(pixels);
  ctx.putImageData(imageData, 0, 0);
  return canvas.toDataURL();
}

async function updateBlurhash() {
  const hash = extractBlurhash(props.item);
  if (hash) {
    try {
      blurhashUrl.value = await decodeBlurhash(hash);
    } catch {
      blurhashUrl.value = '';
    }
  } else {
    blurhashUrl.value = '';
  }
}

watch(
  () => props.item.Id,
  () => {
    imageError.value = false;
    updateBlurhash();
  }
);

onMounted(updateBlurhash);

const imageUrl = computed(() => {
  if (props.thumb) {
    return api.backdropUrl(props.item) || api.itemImageUrl(props.item);
  }
  return api.itemImageUrl(props.item) || api.backdropUrl(props.item);
});

const logoUrl = computed(() => api.logoUrl(props.item));
const showImage = computed(() => Boolean(imageUrl.value) && !imageError.value);

const title = computed(() =>
  props.item.Type === 'Episode' && props.item.SeriesName ? props.item.SeriesName : props.item.Name
);
const secondary = computed(() => props.subtitle || itemSubtitle(props.item));
const playable = computed(() => !props.item.IsFolder && Boolean(props.item.MediaSources?.length));
const fallbackLabel = computed(() => {
  if (props.item.IsFolder) {
    return '目录';
  }
  return (props.item.Name || '').slice(0, 1).toUpperCase() || '?';
});

const isSelected = computed(() => selectedItems.has(props.item.Id));

const progress = computed(() => {
  const ticks = props.item.UserData?.PlaybackPositionTicks ?? 0;
  const runtime = props.item.RunTimeTicks ?? 0;
  if (!ticks || !runtime) return 0;
  return Math.max(0, Math.min(100, (ticks / runtime) * 100));
});

async function doRefreshMetadata() {
  try {
    await api.refreshItemMetadata(props.item.Id, {
      metadataRefreshMode: 'FullRefresh',
      imageRefreshMode: 'FullRefresh',
      replaceAllMetadata: false,
      replaceAllImages: false
    });
    toast.success('元数据刷新已提交');
  } catch (err: any) {
    toast.error('刷新元数据失败: ' + (err?.message || err));
  }
}

async function doDeleteItem() {
  if (!window.confirm(`确定要删除"${props.item.Name}"吗？此操作不可撤销。`)) return;
  try {
    await api.deleteItem(props.item.Id);
    toast.success(`已删除: ${props.item.Name}`);
    emit('deleted', props.item);
  } catch (err: any) {
    toast.error('删除失败: ' + (err?.message || err));
  }
}

function goToDetail() {
  router.push(itemRoute(props.item));
}

const contextMenuItems = computed<ContextMenuItem[][]>(() => {
  const playbackGroup: ContextMenuItem[] = [];
  if (playable.value) {
    playbackGroup.push({ label: '播放', icon: 'i-lucide-play', onSelect: () => emit('play', props.item) });
    playbackGroup.push({ label: '加入队列', icon: 'i-lucide-list-plus', onSelect: () => enqueue(props.item, 'last') });
    playbackGroup.push({ label: '作为下一首', icon: 'i-lucide-play-circle', onSelect: () => enqueue(props.item, 'next') });
  }

  const userGroup: ContextMenuItem[] = [
    {
      label: props.item.UserData?.IsFavorite ? '取消收藏' : '添加到收藏',
      icon: props.item.UserData?.IsFavorite ? 'i-lucide-heart-off' : 'i-lucide-heart',
      onSelect: () => void toggleFavorite(props.item)
    },
    {
      label: props.item.UserData?.Played ? '标记未观看' : '标记为已播放',
      icon: props.item.UserData?.Played ? 'i-lucide-eye-off' : 'i-lucide-check',
      onSelect: () => void togglePlayed(props.item)
    },
    {
      label: isInWatchLater(props.item.Id) ? '移出稍后观看' : '添加到稍后观看',
      icon: 'i-lucide-clock',
      onSelect: () => toggleWatchLater(props.item)
    },
    {
      label: '添加到合集',
      icon: 'i-lucide-folder-plus',
      onSelect: () => {
        collectionDialogItemIds.value = [props.item.Id];
        collectionDialogOpen.value = true;
      }
    },
    {
      label: '添加到播放列表',
      icon: 'i-lucide-list-music',
      onSelect: async () => {
        try {
          const result = await api.listPlaylists();
          const lists = result.Items ?? [];
          if (lists.length > 0) {
            await api.addPlaylistItems(lists[0].Id, [props.item.Id]);
            toast.success('已添加到播放列表');
          } else {
            toast.info('暂无播放列表，请先创建');
          }
        } catch {
          toast.error('添加失败');
        }
      }
    }
  ];

  const adminGroup: ContextMenuItem[] = [];
  if (isAdmin.value) {
    adminGroup.push({
      label: '刷新元数据',
      icon: 'i-lucide-refresh-cw',
      onSelect: doRefreshMetadata
    });
    adminGroup.push({
      label: '编辑元数据',
      icon: 'i-lucide-file-edit',
      onSelect: () => emit('editMetadata', props.item)
    });
    adminGroup.push({
      label: '识别',
      icon: 'i-lucide-search',
      onSelect: () => emit('identify', props.item)
    });
    adminGroup.push({
      label: '编辑图像',
      icon: 'i-lucide-image',
      onSelect: goToDetail
    });
    adminGroup.push({
      label: '删除',
      icon: 'i-lucide-trash-2',
      color: 'error',
      onSelect: doDeleteItem
    });
  }

  const groups: ContextMenuItem[][] = [];
  if (playbackGroup.length) groups.push(playbackGroup);
  groups.push(userGroup);
  if (adminGroup.length) groups.push(adminGroup);
  groups.push([{ label: '查看详情', icon: 'i-lucide-info', onSelect: () => emit('select', props.item) }]);
  return groups;
});

// Shared open for both the hover button dropdown and contextmenu
const showMenu = ref(false);
const menuItems = computed(() => contextMenuItems.value);

function openDropdown(e: MouseEvent) {
  e.preventDefault();
  showMenu.value = true;
}

function openContextMenu(e: MouseEvent) {
  ctxMenu.value?.show(e);
}

function handleCardClick(e: MouseEvent) {
  if (e.ctrlKey || e.metaKey) {
    e.preventDefault();
    enableSelectionMode();
    toggleSelection(props.item.Id);
    return;
  }
  if (selectionMode.value) {
    toggleSelection(props.item.Id);
    return;
  }
  emit('select', props.item);
}
</script>

<template>
  <article
    class="media-card group relative flex cursor-pointer flex-col gap-2 transition"
    :class="isSelected ? 'ring-2 ring-primary rounded-lg' : ''"
    @click="handleCardClick"
    @contextmenu="openContextMenu"
  >
    <div
      class="bg-elevated ring-default group-hover:ring-primary/60 relative overflow-hidden rounded-lg ring-1 transition-all group-hover:-translate-y-0.5 group-hover:shadow-xl"
      :class="[props.thumb ? 'aspect-video' : 'aspect-[2/3]', isSelected ? 'opacity-80' : '']"
    >
      <img
        v-if="blurhashUrl && !showImage"
        :src="blurhashUrl"
        :alt="props.item.Name"
        class="absolute inset-0 h-full w-full object-cover"
      />
      <img
        v-if="showImage"
        :src="imageUrl"
        :alt="props.item.Name"
        loading="lazy"
        decoding="async"
        class="h-full w-full object-cover transition-transform duration-300 group-hover:scale-[1.04]"
        @error="imageError = true"
      />
      <div
        v-else-if="!blurhashUrl"
        class="from-primary/20 to-primary/5 text-primary flex h-full w-full items-center justify-center bg-gradient-to-br text-2xl font-bold"
      >
        {{ fallbackLabel }}
      </div>

      <!-- Logo clearart 悬浮覆盖 -->
      <div
        v-if="logoUrl"
        class="pointer-events-none absolute inset-x-2 bottom-8 hidden sm:block"
      >
        <img
          :src="logoUrl"
          :alt="props.item.Name"
          class="max-h-10 w-auto drop-shadow-[0_1px_4px_rgba(0,0,0,0.8)] opacity-0 transition-opacity duration-300 group-hover:opacity-100"
        />
      </div>

      <!-- 选择复选框 -->
      <div
        v-if="selectionMode"
        class="absolute left-2 top-2 z-10"
        @click.stop="toggleSelection(props.item.Id)"
      >
        <div
          class="flex h-6 w-6 items-center justify-center rounded-md border-2 transition-colors"
          :class="isSelected ? 'border-primary bg-primary text-primary-contrast' : 'border-white/70 bg-black/40'"
        >
          <UIcon v-if="isSelected" name="i-lucide-check" class="size-4" />
        </div>
      </div>

      <!-- 质量角标 -->
      <div class="absolute left-2 flex flex-col items-start gap-1" :class="selectionMode ? 'top-10' : 'top-2'">
        <MediaQualityBadges :item="props.item" compact />
      </div>

      <!-- 进度条 -->
      <div v-if="progress > 0" class="absolute inset-x-0 bottom-0 h-1 bg-black/40">
        <div class="bg-primary h-full" :style="{ width: `${progress}%` }" />
      </div>

      <!-- 悬浮播放 -->
      <button
        v-if="playable"
        type="button"
        title="播放"
        class="absolute inset-0 flex items-center justify-center bg-gradient-to-t from-black/70 via-black/20 to-transparent opacity-0 transition-opacity group-hover:opacity-100"
        @click.stop="emit('play', props.item)"
      >
        <span
          class="bg-primary text-primary-contrast flex h-12 w-12 items-center justify-center rounded-full shadow-lg ring-4 ring-black/20"
        >
          <UIcon name="i-lucide-play" class="size-6" />
        </span>
      </button>

      <!-- 右上角状态 -->
      <span
        v-if="props.item.UserData?.Played"
        class="bg-primary text-primary-contrast absolute right-2 top-2 inline-flex h-6 w-6 items-center justify-center rounded-full text-xs"
      >
        <UIcon name="i-lucide-check" class="size-4" />
      </span>
      <span
        v-else-if="props.item.UserData?.UnplayedItemCount"
        class="bg-primary text-primary-contrast absolute right-2 top-2 inline-flex min-w-6 items-center justify-center rounded-full px-1.5 py-0.5 text-[10px] font-bold"
      >
        {{ props.item.UserData.UnplayedItemCount }}
      </span>
      <span
        v-else-if="props.item.UserData?.IsFavorite"
        class="bg-error absolute right-2 top-2 inline-flex h-6 w-6 items-center justify-center rounded-full text-xs text-white"
      >
        <UIcon name="i-lucide-heart" class="size-3.5" />
      </span>

      <!-- 更多按钮 -->
      <UDropdownMenu v-model:open="showMenu" :items="menuItems">
        <UButton
          icon="i-lucide-more-horizontal"
          color="neutral"
          variant="solid"
          size="xs"
          class="absolute bottom-2 right-2 opacity-0 transition-opacity group-hover:opacity-100"
          aria-label="更多"
          @click.stop
        />
      </UDropdownMenu>
    </div>

    <!-- 右键上下文菜单 -->
    <ContextMenu
      ref="ctxMenu"
      :items="contextMenuItems"
      :preview-image="imageUrl || undefined"
      :preview-title="title"
      :preview-subtitle="secondary || undefined"
    />

    <CollectionEditorDialog
      :item-ids="collectionDialogItemIds"
      v-model:open="collectionDialogOpen"
    />

    <div class="min-w-0 space-y-0.5 px-0.5">
      <h3
        class="text-highlighted group-hover:text-primary truncate text-sm font-medium transition-colors"
        :title="title"
      >
        {{ title }}
      </h3>
      <p class="text-muted truncate text-xs" :title="secondary">{{ secondary }}</p>
    </div>
  </article>
</template>

<style scoped>
.media-card {
  content-visibility: auto;
  contain-intrinsic-size: auto 0 auto 280px;
}
</style>
