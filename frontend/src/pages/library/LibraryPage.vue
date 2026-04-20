<script setup lang="ts">
import { computed, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../../components/MediaCard.vue';
import { api, backToParent, currentParentName, items, loadItems, parentStack, selectedLibrary, selectLibrary, state } from '../../store/app';
import type { BaseItemDto } from '../../api/emby';

const route = useRoute();
const router = useRouter();

const viewTypes = [
  { label: '全部', value: '' },
  { label: '电影', value: 'Movie' },
  { label: '剧集', value: 'Series' },
  { label: '季', value: 'Season' },
  { label: '单集', value: 'Episode' },
  { label: '文件夹', value: 'Folder' }
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

const breadcrumbs = computed(() => [selectedLibrary.value?.Name, ...parentStack.value.map((item) => item.Name)].filter(Boolean));

async function openMedia(item: BaseItemDto) {
  if (item.Type === 'CollectionFolder') {
    await router.push(`/library/${item.Id}`);
    return;
  }

  if (item.IsFolder) {
    parentStack.value.push(item);
    await loadItems();
    return;
  }

  await router.push(`/item/${item.Id}`);
}

function playItem(item: BaseItemDto) {
  window.open(api.streamUrl(item), '_blank', 'noopener');
}

function toggleSortOrder() {
  state.librarySortAscending = !state.librarySortAscending;
}
</script>

<template>
  <section class="home-sections">
    <nav class="crumbs">
      <button v-if="parentStack.length" type="button" title="返回上一级" @click="backToParent">‹</button>
      <span v-for="crumb in breadcrumbs" :key="crumb">{{ crumb }}</span>
    </nav>

    <section class="media-row">
      <div class="section-heading">
        <div>
          <h3>{{ currentParentName }}</h3>
          <span>{{ items.length }} 个条目</span>
        </div>
        <div class="button-row">
          <select v-model="state.libraryViewType">
            <option v-for="option in viewTypes" :key="option.value || 'all'" :value="option.value">
              {{ option.label }}
            </option>
          </select>
          <select v-model="state.librarySortBy">
            <option value="SortName">名称</option>
            <option value="DateCreated">添加时间</option>
            <option value="ProductionYear">年份</option>
            <option value="IndexNumber">集数</option>
          </select>
          <button class="secondary" type="button" @click="toggleSortOrder">
            {{ state.librarySortAscending ? '升序' : '降序' }}
          </button>
        </div>
      </div>

      <div v-if="items.length" class="poster-grid">
        <MediaCard
          v-for="item in items"
          :key="item.Id"
          :item="item"
          @play="playItem"
          @select="openMedia"
        />
      </div>
      <div v-else class="empty">
        <p>{{ selectedLibrary?.Name || '媒体库' }}</p>
        <h2>这里暂时没有内容</h2>
        <p>可以尝试切换排序、清空筛选，或者先在管理员页面执行一次媒体扫描。</p>
      </div>
    </section>
  </section>
</template>
