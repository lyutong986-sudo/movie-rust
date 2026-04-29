<script setup lang="ts">
import { ref, watch } from 'vue';
import type { BaseItemDto } from '../api/emby';
import { api } from '../store/app';
import { useAppToast } from '../composables/toast';

const props = defineProps<{
  item: BaseItemDto;
  open: boolean;
}>();

const emit = defineEmits<{
  'update:open': [value: boolean];
  identified: [];
}>();

const toast = useAppToast();

const searchName = ref('');
const searchYear = ref<number | undefined>();
const searchTmdb = ref('');
const searchImdb = ref('');
const results = ref<any[]>([]);
const searching = ref(false);
const applying = ref<number | null>(null);

const dialogOpen = ref(props.open);
watch(() => props.open, (v) => { dialogOpen.value = v; });
watch(dialogOpen, (v) => emit('update:open', v));

watch(() => props.open, (v) => {
  if (v) {
    searchName.value = props.item.Name || '';
    searchYear.value = props.item.ProductionYear || undefined;
    searchTmdb.value = props.item.ProviderIds?.Tmdb || '';
    searchImdb.value = props.item.ProviderIds?.Imdb || '';
    results.value = [];
    searching.value = false;
    applying.value = null;
  }
}, { immediate: true });

async function doSearch() {
  if (!searchName.value.trim() && !searchTmdb.value.trim() && !searchImdb.value.trim()) return;
  searching.value = true;
  results.value = [];
  try {
    const providerIds: Record<string, string> = {};
    if (searchTmdb.value.trim()) providerIds.Tmdb = searchTmdb.value.trim();
    if (searchImdb.value.trim()) providerIds.Imdb = searchImdb.value.trim();

    const query = {
      SearchInfo: {
        Name: searchName.value.trim(),
        ...(searchYear.value ? { Year: searchYear.value } : {}),
        ...(Object.keys(providerIds).length ? { ProviderIds: providerIds } : {})
      }
    };

    if (props.item.Type === 'Series') {
      results.value = await api.remoteSearchSeries(query);
    } else {
      results.value = await api.remoteSearchMovie(query);
    }
  } catch (err) {
    toast.error(err instanceof Error ? err.message : '搜索失败');
  } finally {
    searching.value = false;
  }
}

async function applyResult(result: any, index: number) {
  if (applying.value !== null) return;
  applying.value = index;
  try {
    await api.remoteSearchApply(props.item.Id, result);
    toast.success('识别成功，元数据已更新');
    dialogOpen.value = false;
    emit('identified');
  } catch (err) {
    toast.error(err instanceof Error ? err.message : '应用失败');
  } finally {
    applying.value = null;
  }
}

function truncate(text: string, max: number) {
  if (!text) return '';
  return text.length > max ? text.slice(0, max) + '…' : text;
}
</script>

<template>
  <UModal v-model:open="dialogOpen" :ui="{ content: 'max-w-2xl' }">
    <template #header>
      <h3 class="text-highlighted text-base font-semibold">识别</h3>
    </template>
    <template #body>
      <div class="space-y-4">
        <div class="grid grid-cols-2 gap-3">
          <UFormField label="名称" class="col-span-2">
            <UInput v-model="searchName" placeholder="名称" class="w-full" @keydown.enter="doSearch" />
          </UFormField>
          <UFormField label="年份">
            <UInput v-model.number="searchYear" type="number" placeholder="年份" class="w-full" @keydown.enter="doSearch" />
          </UFormField>
          <UFormField label="TMDB ID">
            <UInput v-model="searchTmdb" placeholder="TMDB ID" class="w-full" @keydown.enter="doSearch" />
          </UFormField>
          <UFormField label="IMDB ID">
            <UInput v-model="searchImdb" placeholder="IMDB ID" class="w-full" @keydown.enter="doSearch" />
          </UFormField>
          <div class="flex items-end">
            <UButton icon="i-lucide-search" :loading="searching" @click="doSearch">
              搜索
            </UButton>
          </div>
        </div>

        <div v-if="searching" class="flex flex-col items-center gap-2 py-8">
          <UProgress animation="carousel" class="w-48" />
          <p class="text-muted text-sm">正在搜索…</p>
        </div>

        <div v-else-if="results.length" class="max-h-[28rem] space-y-2 overflow-y-auto">
          <div
            v-for="(r, idx) in results"
            :key="idx"
            class="border-default hover:bg-elevated/40 flex gap-3 rounded-lg border p-3 transition"
          >
            <img
              v-if="r.ImageUrl"
              :src="r.ImageUrl"
              alt=""
              class="bg-elevated h-28 w-20 shrink-0 rounded object-cover"
            />
            <div
              v-else
              class="bg-elevated/50 text-muted flex h-28 w-20 shrink-0 items-center justify-center rounded text-xs"
            >
              无图
            </div>
            <div class="min-w-0 flex-1">
              <p class="text-highlighted text-sm font-medium">
                {{ r.Name }}
                <span v-if="r.ProductionYear" class="text-muted text-xs">({{ r.ProductionYear }})</span>
              </p>
              <p v-if="r.Overview" class="text-muted mt-1 text-xs leading-relaxed">
                {{ truncate(r.Overview, 150) }}
              </p>
              <div v-if="r.ProviderIds" class="mt-1 flex flex-wrap gap-2">
                <UBadge
                  v-for="(val, key) in r.ProviderIds"
                  :key="String(key)"
                  size="xs"
                  color="neutral"
                  variant="outline"
                >
                  {{ key }}: {{ val }}
                </UBadge>
              </div>
            </div>
            <div class="flex shrink-0 items-start">
              <UButton
                size="sm"
                :loading="applying === idx"
                :disabled="applying !== null"
                @click="applyResult(r, idx)"
              >
                应用
              </UButton>
            </div>
          </div>
        </div>

        <div v-else class="text-muted py-8 text-center text-sm">
          <UIcon name="i-lucide-search" class="mx-auto mb-2 size-8 opacity-50" />
          <p>输入名称后点击搜索</p>
        </div>
      </div>
    </template>
  </UModal>
</template>
