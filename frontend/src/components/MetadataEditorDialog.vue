<script setup lang="ts">
import { ref, watch } from 'vue';
import type { BaseItemDto, ExternalIdInfo } from '../api/emby';
import { api, isAdmin } from '../store/app';
import { useAppToast } from '../composables/toast';

const props = defineProps<{
  item: BaseItemDto;
  open: boolean;
}>();

const emit = defineEmits<{
  'update:open': [value: boolean];
  saved: [];
}>();

const toast = useAppToast();
const saving = ref(false);
const externalIdInfos = ref<ExternalIdInfo[]>([]);
const editorLoading = ref(false);

const name = ref('');
const originalTitle = ref('');
const sortName = ref('');
const overview = ref('');
const productionYear = ref<number | undefined>();
const premiereDate = ref('');
const officialRating = ref('');
const communityRating = ref<number | undefined>();
const genres = ref('');
const tags = ref('');
const studios = ref('');
const providerIds = ref<Record<string, string>>({});

function populateForm() {
  const it = props.item;
  name.value = it.Name || '';
  originalTitle.value = it.OriginalTitle || '';
  sortName.value = it.SortName || '';
  overview.value = it.Overview || '';
  productionYear.value = it.ProductionYear;
  premiereDate.value = it.PremiereDate ? it.PremiereDate.slice(0, 10) : '';
  officialRating.value = it.OfficialRating || '';
  communityRating.value = it.CommunityRating;
  genres.value = (it.Genres || []).join(', ');
  tags.value = (it.Tags || []).join(', ');
  studios.value = (it.Studios || []).map((s) => s.Name).join(', ');
  providerIds.value = { ...(it.ProviderIds || {}) };
}

watch(
  () => props.open,
  async (open) => {
    if (!open) return;
    populateForm();
    editorLoading.value = true;
    try {
      const editor = await api.getMetadataEditor(props.item.Id);
      externalIdInfos.value = editor.ExternalIdInfos || [];
    } catch {
      externalIdInfos.value = [];
    } finally {
      editorLoading.value = false;
    }
  }
);

function splitCsv(value: string): string[] {
  return value
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
}

async function submit() {
  if (!isAdmin.value || saving.value) return;
  saving.value = true;
  try {
    const body: Partial<BaseItemDto> = {
      Id: props.item.Id,
      Name: name.value,
      OriginalTitle: originalTitle.value,
      SortName: sortName.value,
      Overview: overview.value,
      ProductionYear: productionYear.value || undefined,
      PremiereDate: premiereDate.value || undefined,
      OfficialRating: officialRating.value || undefined,
      CommunityRating: communityRating.value || undefined,
      Genres: splitCsv(genres.value),
      Tags: splitCsv(tags.value),
      Studios: splitCsv(studios.value).map((s) => ({ Name: s })),
      ProviderIds: { ...providerIds.value }
    };
    await api.updateItem(props.item.Id, body);
    toast.success('元数据已保存');
    emit('saved');
    emit('update:open', false);
  } catch (err) {
    toast.error(err instanceof Error ? err.message : '保存失败');
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <UModal :open="open" :ui="{ content: 'max-w-2xl' }" @update:open="emit('update:open', $event)">
    <template #header>
      <h3 class="text-highlighted text-base font-semibold">编辑元数据</h3>
    </template>
    <template #body>
      <form v-if="isAdmin" class="space-y-4" @submit.prevent="submit">
        <div class="grid gap-4 sm:grid-cols-2">
          <div class="sm:col-span-2">
            <label class="text-muted mb-1 block text-xs font-medium">名称</label>
            <UInput v-model="name" class="w-full" />
          </div>
          <div>
            <label class="text-muted mb-1 block text-xs font-medium">原始标题</label>
            <UInput v-model="originalTitle" class="w-full" />
          </div>
          <div>
            <label class="text-muted mb-1 block text-xs font-medium">排序名称</label>
            <UInput v-model="sortName" class="w-full" />
          </div>
          <div class="sm:col-span-2">
            <label class="text-muted mb-1 block text-xs font-medium">简介</label>
            <UTextarea v-model="overview" :rows="4" class="w-full" />
          </div>
          <div>
            <label class="text-muted mb-1 block text-xs font-medium">制作年份</label>
            <UInput v-model.number="productionYear" type="number" class="w-full" />
          </div>
          <div>
            <label class="text-muted mb-1 block text-xs font-medium">首播日期</label>
            <UInput v-model="premiereDate" type="date" class="w-full" />
          </div>
          <div>
            <label class="text-muted mb-1 block text-xs font-medium">分级</label>
            <UInput v-model="officialRating" placeholder="PG-13" class="w-full" />
          </div>
          <div>
            <label class="text-muted mb-1 block text-xs font-medium">社区评分</label>
            <UInput
              v-model.number="communityRating"
              type="number"
              step="0.1"
              min="0"
              max="10"
              class="w-full"
            />
          </div>
          <div class="sm:col-span-2">
            <label class="text-muted mb-1 block text-xs font-medium">类型</label>
            <UInput v-model="genres" placeholder="逗号分隔，如: 动作, 科幻" class="w-full" />
          </div>
          <div class="sm:col-span-2">
            <label class="text-muted mb-1 block text-xs font-medium">标签</label>
            <UInput v-model="tags" placeholder="逗号分隔" class="w-full" />
          </div>
          <div class="sm:col-span-2">
            <label class="text-muted mb-1 block text-xs font-medium">工作室</label>
            <UInput v-model="studios" placeholder="逗号分隔" class="w-full" />
          </div>
        </div>

        <div v-if="externalIdInfos.length" class="space-y-3">
          <h4 class="text-highlighted text-sm font-semibold">外部ID</h4>
          <div class="grid gap-3 sm:grid-cols-2">
            <div v-for="info in externalIdInfos" :key="info.Key">
              <label class="text-muted mb-1 block text-xs font-medium">{{ info.Name }}</label>
              <UInput v-model="providerIds[info.Key]" class="w-full" />
            </div>
          </div>
        </div>
        <div v-else-if="editorLoading" class="text-muted text-sm">正在加载外部ID信息…</div>

        <div class="flex justify-end gap-2 pt-2">
          <UButton
            color="neutral"
            variant="subtle"
            @click="emit('update:open', false)"
          >
            取消
          </UButton>
          <UButton type="submit" :loading="saving">
            保存
          </UButton>
        </div>
      </form>
      <p v-else class="text-muted text-sm">仅管理员可编辑元数据。</p>
    </template>
  </UModal>
</template>
