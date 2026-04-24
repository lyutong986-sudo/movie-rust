<script setup lang="ts">
import { computed } from 'vue';
import {
  completeWizard,
  createWizardAdmin,
  saveLanguageAndContinue,
  saveMetadataAndContinue,
  state
} from '../store/app';

const steps = [
  { id: 1, label: '语言', icon: 'i-lucide-languages' },
  { id: 2, label: '管理员', icon: 'i-lucide-user-cog' },
  { id: 3, label: '元数据', icon: 'i-lucide-tags' },
  { id: 4, label: '远程访问', icon: 'i-lucide-globe' }
];

const progress = computed(() => ((state.wizardStep - 1) / (steps.length - 1)) * 100);

const CULTURES = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'en-US', label: 'English' }
];

const METADATA_LANGS = [
  { value: 'zh', label: '中文' },
  { value: 'en', label: 'English' },
  { value: 'ja', label: '日本語' },
  { value: 'ko', label: '한국어' }
];

const METADATA_COUNTRIES = [
  { value: 'CN', label: '中国' },
  { value: 'US', label: 'United States' },
  { value: 'JP', label: '日本' },
  { value: 'KR', label: '韩国' }
];
</script>

<template>
  <div class="space-y-6">
    <header>
      <p class="text-muted text-xs">首次启动向导</p>
      <h2 class="text-highlighted text-xl font-semibold">欢迎使用 {{ state.serverName }}</h2>
    </header>

    <!-- 步骤指示器 -->
    <div class="space-y-2">
      <UProgress :model-value="progress" size="sm" />
      <div class="flex justify-between">
        <div
          v-for="step in steps"
          :key="step.id"
          class="flex flex-col items-center gap-1"
        >
          <div
            class="flex h-8 w-8 items-center justify-center rounded-full text-xs font-medium transition"
            :class="
              state.wizardStep > step.id
                ? 'bg-primary text-primary-contrast'
                : state.wizardStep === step.id
                  ? 'bg-primary/20 text-primary ring-2 ring-primary'
                  : 'bg-elevated text-muted'
            "
          >
            <UIcon v-if="state.wizardStep > step.id" name="i-lucide-check" class="size-4" />
            <UIcon v-else :name="step.icon" class="size-4" />
          </div>
          <span class="text-muted text-[11px]">{{ step.label }}</span>
        </div>
      </div>
    </div>

    <!-- 步骤 1：语言 -->
    <section v-if="state.wizardStep === 1" class="space-y-4">
      <h3 class="text-highlighted text-base font-semibold">选择你的媒体服务器语言</h3>
      <p class="text-muted text-sm">
        当前界面使用简体中文，后端会按 Jellyfin/Emby 的 Startup 配置接口保存首选语言。
      </p>
      <UFormField label="显示语言">
        <USelect v-model="state.uiCulture" :items="CULTURES" class="w-full" />
      </UFormField>
      <div class="flex justify-end">
        <UButton :loading="state.busy" icon="i-lucide-arrow-right" trailing @click="saveLanguageAndContinue">
          继续
        </UButton>
      </div>
    </section>

    <!-- 步骤 2：管理员 -->
    <form v-else-if="state.wizardStep === 2" class="space-y-4" @submit.prevent="createWizardAdmin">
      <h3 class="text-highlighted text-base font-semibold">创建管理员账户</h3>
      <UAlert
        v-if="state.adminCreated"
        color="primary"
        variant="subtle"
        icon="i-lucide-check"
        title="管理员账户已经创建，可以继续设置元数据。"
      />
      <UFormField label="用户名" required>
        <UInput
          v-model="state.adminName"
          autocomplete="username"
          :disabled="state.adminCreated"
          icon="i-lucide-user"
          class="w-full"
        />
      </UFormField>
      <UFormField label="密码" required>
        <UInput
          v-model="state.adminPassword"
          :type="state.showWizardPassword ? 'text' : 'password'"
          autocomplete="new-password"
          :disabled="state.adminCreated"
          icon="i-lucide-lock"
          class="w-full"
        >
          <template #trailing>
            <UButton
              color="neutral"
              variant="link"
              size="sm"
              :icon="state.showWizardPassword ? 'i-lucide-eye-off' : 'i-lucide-eye'"
              @click="state.showWizardPassword = !state.showWizardPassword"
            />
          </template>
        </UInput>
      </UFormField>
      <UFormField label="确认密码" required>
        <UInput
          v-model="state.adminPasswordConfirm"
          :type="state.showWizardPassword ? 'text' : 'password'"
          autocomplete="new-password"
          :disabled="state.adminCreated"
          icon="i-lucide-lock"
          class="w-full"
        />
      </UFormField>
      <div class="flex justify-between pt-2">
        <UButton color="neutral" variant="ghost" icon="i-lucide-arrow-left" @click="state.wizardStep = 1">
          返回
        </UButton>
        <UButton type="submit" :loading="state.busy" icon="i-lucide-arrow-right" trailing>
          继续
        </UButton>
      </div>
    </form>

    <!-- 步骤 3：元数据 -->
    <form v-else-if="state.wizardStep === 3" class="space-y-4" @submit.prevent="saveMetadataAndContinue">
      <h3 class="text-highlighted text-base font-semibold">首选元数据语言</h3>
      <p class="text-muted text-sm">
        这一项对应 Jellyfin 的元数据语言配置，后续扫描和识别媒体时会沿用这些首选项。
      </p>
      <div class="grid gap-4 sm:grid-cols-2">
        <UFormField label="元数据语言">
          <USelect v-model="state.metadataLanguage" :items="METADATA_LANGS" class="w-full" />
        </UFormField>
        <UFormField label="国家/地区">
          <USelect v-model="state.metadataCountry" :items="METADATA_COUNTRIES" class="w-full" />
        </UFormField>
      </div>
      <div class="flex justify-between pt-2">
        <UButton color="neutral" variant="ghost" icon="i-lucide-arrow-left" @click="state.wizardStep = 2">
          返回
        </UButton>
        <UButton type="submit" :loading="state.busy" icon="i-lucide-arrow-right" trailing>
          继续
        </UButton>
      </div>
    </form>

    <!-- 步骤 4：远程访问 -->
    <section v-else class="space-y-4">
      <h3 class="text-highlighted text-base font-semibold">远程访问</h3>
      <p class="text-muted text-sm">
        保留 Jellyfin/Emby 的远程访问配置入口。当前版本会保存选择，实际端口映射可以在部署层继续配置。
      </p>
      <div class="space-y-3">
        <USwitch v-model="state.allowRemoteAccess" label="允许远程连接到服务器" />
        <USwitch v-model="state.enableUPNP" label="自动端口映射 (UPnP)" />
      </div>
      <div class="flex justify-between pt-2">
        <UButton color="neutral" variant="ghost" icon="i-lucide-arrow-left" @click="state.wizardStep = 3">
          返回
        </UButton>
        <UButton :loading="state.busy" icon="i-lucide-check" @click="completeWizard">
          完成设置
        </UButton>
      </div>
    </section>
  </div>
</template>
