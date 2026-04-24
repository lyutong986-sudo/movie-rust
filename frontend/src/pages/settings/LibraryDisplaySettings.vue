<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import type { LibraryDisplayConfiguration } from '../../api/emby';
import { api, isAdmin } from '../../store/app';

const loading = ref(true);
const saving = ref(false);
const error = ref('');
const saved = ref('');

const form = reactive<LibraryDisplayConfiguration>({
  DisplayFolderView: false,
  DisplaySpecialsWithinSeasons: true,
  GroupMoviesIntoCollections: true,
  DisplayCollectionsView: true,
  EnableExternalContentInSuggestions: false,
  DateAddedBehavior: 0,
  MetadataPath: '',
  SaveMetadataHidden: false,
  SeasonZeroDisplayName: 'Specials',
  FanartApiKey: ''
});

const dateAddedOptions = [
  { label: '导入时间', value: 0 },
  { label: '文件创建时间', value: 1 }
];

onMounted(async () => {
  if (!isAdmin.value) {
    loading.value = false;
    return;
  }
  try {
    Object.assign(form, await api.libraryDisplayConfiguration());
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
});

async function save() {
  error.value = '';
  saved.value = '';
  saving.value = true;
  try {
    Object.assign(form, await api.updateLibraryDisplayConfiguration(form));
    saved.value = '媒体库显示配置已保存';
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <SettingsLayout>
    <div v-if="!isAdmin" class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center">
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能修改媒体库显示配置。</p>
    </div>

    <form v-else class="space-y-4" @submit.prevent="save">
      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
      <UAlert v-else-if="saved" color="success" icon="i-lucide-check" :description="saved" />

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">显示选项</h3>
        </template>
        <div class="grid gap-3 sm:grid-cols-2">
          <USwitch v-model="form.DisplayFolderView" label="显示文件夹视图" />
          <USwitch v-model="form.DisplaySpecialsWithinSeasons" label="将 Specials 归入正常季" />
          <USwitch v-model="form.GroupMoviesIntoCollections" label="电影自动归入合集" />
          <USwitch v-model="form.DisplayCollectionsView" label="显示合集独立视图" />
          <USwitch v-model="form.EnableExternalContentInSuggestions" label="推荐中包含外部内容" />
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">日期与元数据</h3>
        </template>
        <div class="grid gap-4 sm:grid-cols-2">
          <UFormField label="DateAdded 取值方式" hint="决定媒体条目的添加时间来源">
            <USelect v-model="form.DateAddedBehavior" :items="dateAddedOptions" value-key="value" class="w-full" />
          </UFormField>
          <UFormField label="Season 0 显示名">
            <UInput v-model="form.SeasonZeroDisplayName" class="w-full" />
          </UFormField>
          <UFormField label="全局元数据路径" hint="留空则使用默认位置">
            <UInput v-model.trim="form.MetadataPath" placeholder="/var/lib/movie-rust/metadata" class="w-full" />
          </UFormField>
          <UFormField label="保存元数据为隐藏文件">
            <USwitch v-model="form.SaveMetadataHidden" />
          </UFormField>
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">外部元数据</h3>
        </template>
        <UFormField label="Fanart.tv API Key" hint="用于获取高质量海报/背景图">
          <UInput v-model.trim="form.FanartApiKey" placeholder="在 fanart.tv 生成你的 Key" class="w-full" />
        </UFormField>
        <template #footer>
          <div class="flex justify-end">
            <UButton type="submit" :loading="saving" icon="i-lucide-save">保存媒体库显示配置</UButton>
          </div>
        </template>
      </UCard>
    </form>
  </SettingsLayout>
</template>
