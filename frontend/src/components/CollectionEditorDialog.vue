<script setup lang="ts">
import { ref, watch } from 'vue';
import type { BaseItemDto } from '../api/emby';
import { api } from '../store/app';
import { useAppToast } from '../composables/toast';

const props = defineProps<{
  itemIds: string[];
  open: boolean;
}>();

const emit = defineEmits<{
  'update:open': [value: boolean];
  saved: [];
}>();

const toast = useAppToast();
const collections = ref<BaseItemDto[]>([]);
const loadingCollections = ref(false);
const submitting = ref(false);
const newCollectionName = ref('');

const dialogOpen = ref(props.open);

watch(
  () => props.open,
  (val) => {
    dialogOpen.value = val;
    if (val) {
      fetchCollections();
    }
  }
);

watch(dialogOpen, (val) => {
  emit('update:open', val);
});

async function fetchCollections() {
  loadingCollections.value = true;
  try {
    const result = await api.getCollections();
    collections.value = result.Items || [];
  } catch {
    collections.value = [];
  } finally {
    loadingCollections.value = false;
  }
}

async function addToExisting(collectionId: string) {
  if (submitting.value) return;
  submitting.value = true;
  try {
    await api.addCollectionItems(collectionId, props.itemIds);
    toast.success('已添加到合集');
    emit('saved');
    dialogOpen.value = false;
  } catch (err) {
    toast.error(err instanceof Error ? err.message : '添加失败');
  } finally {
    submitting.value = false;
  }
}

async function createNew() {
  const name = newCollectionName.value.trim();
  if (!name) {
    toast.error('请输入合集名称');
    return;
  }
  if (submitting.value) return;
  submitting.value = true;
  try {
    await api.createCollection(name, props.itemIds);
    toast.success(`已创建合集「${name}」`);
    newCollectionName.value = '';
    emit('saved');
    dialogOpen.value = false;
  } catch (err) {
    toast.error(err instanceof Error ? err.message : '创建失败');
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <UModal v-model:open="dialogOpen" :ui="{ content: 'max-w-lg' }">
    <template #header>
      <h3 class="text-highlighted text-base font-semibold">添加到合集</h3>
    </template>
    <template #body>
      <div class="space-y-4">
        <div>
          <p class="text-highlighted mb-2 text-sm font-medium">添加到现有合集</p>
          <div v-if="loadingCollections" class="text-muted text-center text-sm py-4">加载中...</div>
          <div v-else-if="collections.length" class="max-h-60 space-y-2 overflow-y-auto">
            <button
              v-for="col in collections"
              :key="col.Id"
              type="button"
              class="border-default hover:bg-elevated/60 flex w-full items-center justify-between rounded-lg border p-3 text-start transition"
              :disabled="submitting"
              @click="addToExisting(col.Id)"
            >
              <div class="min-w-0">
                <p class="text-highlighted truncate text-sm font-medium">{{ col.Name }}</p>
                <p v-if="col.ChildCount" class="text-muted text-xs">{{ col.ChildCount }} 个条目</p>
              </div>
              <UIcon name="i-lucide-plus" class="text-primary size-4 shrink-0" />
            </button>
          </div>
          <p v-else class="text-muted border-default bg-elevated/30 rounded-lg border p-3 text-sm">
            暂无合集，使用下方表单创建一个。
          </p>
        </div>

        <USeparator />

        <div>
          <p class="text-highlighted mb-2 text-sm font-medium">创建新合集</p>
          <div class="flex gap-2">
            <UInput
              v-model="newCollectionName"
              placeholder="合集名称"
              class="flex-1"
              @keydown.enter="createNew"
            />
            <UButton
              icon="i-lucide-folder-plus"
              :loading="submitting"
              @click="createNew"
            >
              创建
            </UButton>
          </div>
        </div>
      </div>
    </template>
  </UModal>
</template>
