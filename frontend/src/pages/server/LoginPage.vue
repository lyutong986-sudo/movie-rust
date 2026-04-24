<script setup lang="ts">
import { useRouter } from 'vue-router';
import { api, currentServer, login, publicUsers, state, user } from '../../store/app';

const router = useRouter();

async function submitLogin(name = state.username, password = state.password) {
  await login(name, password);
  if (user.value) {
    await router.replace('/');
  }
}

function pickUser(name: string) {
  state.username = name;
  state.password = '';
  state.loginAsOther = true;
}
</script>

<template>
  <div class="space-y-6">
    <header class="space-y-1">
      <p class="text-muted text-xs">{{ currentServer?.Url || '当前服务器' }}</p>
      <h2 class="text-highlighted text-xl font-semibold">登录</h2>
    </header>

    <!-- 用户选择器（Netflix / Jellyfin 风格的 Profile Picker） -->
    <div v-if="publicUsers.length && !state.loginAsOther" class="space-y-5">
      <p class="text-muted text-center text-sm">选择一位用户登录</p>
      <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4">
        <button
          v-for="publicUser in publicUsers"
          :key="publicUser.Id"
          type="button"
          class="group flex flex-col items-center gap-3 rounded-xl p-3 transition hover:scale-[1.04] focus-visible:outline-2 focus-visible:outline-primary"
          @click="pickUser(publicUser.Name)"
        >
          <div
            class="ring-default group-hover:ring-primary relative size-24 overflow-hidden rounded-2xl ring-2 transition-all sm:size-28"
          >
            <img
              v-if="api.userImageUrl(publicUser)"
              :src="api.userImageUrl(publicUser)"
              :alt="publicUser.Name"
              class="h-full w-full object-cover"
            />
            <div
              v-else
              class="from-primary/30 to-primary/5 text-primary display-font flex h-full w-full items-center justify-center bg-gradient-to-br text-4xl font-bold"
            >
              {{ publicUser.Name.slice(0, 1).toUpperCase() }}
            </div>
            <div
              v-if="publicUser.HasPassword"
              class="bg-background/90 text-muted absolute bottom-1.5 right-1.5 flex size-6 items-center justify-center rounded-full ring-1 ring-default"
            >
              <UIcon name="i-lucide-lock" class="size-3" />
            </div>
          </div>
          <span class="text-highlighted truncate text-sm font-medium group-hover:text-primary">
            {{ publicUser.Name }}
          </span>
        </button>
      </div>

      <div class="flex flex-wrap gap-2 pt-2">
        <UButton color="neutral" variant="subtle" icon="i-lucide-key" @click="state.loginAsOther = true">
          手动输入
        </UButton>
        <UButton color="neutral" variant="ghost" icon="i-lucide-server" @click="router.push('/server/select')">
          切换服务器
        </UButton>
        <UButton color="neutral" variant="ghost" icon="i-lucide-plus" @click="router.push('/server/add')">
          添加服务器
        </UButton>
      </div>
    </div>

    <!-- 登录表单 -->
    <form v-else class="space-y-4" @submit.prevent="submitLogin()">
      <UFormField label="用户名" required>
        <UInput
          v-model="state.username"
          autocomplete="username"
          icon="i-lucide-user"
          class="w-full"
        />
      </UFormField>
      <UFormField label="密码" required>
        <UInput
          v-model="state.password"
          :type="state.showLoginPassword ? 'text' : 'password'"
          autocomplete="current-password"
          icon="i-lucide-lock"
          class="w-full"
          :ui="{ trailing: 'pe-1' }"
        >
          <template #trailing>
            <UButton
              color="neutral"
              variant="link"
              size="sm"
              :icon="state.showLoginPassword ? 'i-lucide-eye-off' : 'i-lucide-eye'"
              :aria-label="state.showLoginPassword ? '隐藏密码' : '显示密码'"
              @click="state.showLoginPassword = !state.showLoginPassword"
            />
          </template>
        </UInput>
      </UFormField>

      <div class="flex flex-wrap items-center gap-2 pt-2">
        <UButton
          v-if="publicUsers.length"
          color="neutral"
          variant="ghost"
          icon="i-lucide-arrow-left"
          @click="state.loginAsOther = false"
        >
          返回
        </UButton>
        <UButton color="neutral" variant="ghost" icon="i-lucide-server" @click="router.push('/server/select')">
          服务器
        </UButton>
        <UButton
          color="neutral"
          variant="ghost"
          icon="i-lucide-key-round"
          @click="router.push('/server/forgot-password')"
        >
          忘记密码
        </UButton>
        <UButton type="submit" :loading="state.busy" class="ms-auto" icon="i-lucide-log-in">
          登录
        </UButton>
      </div>
    </form>
  </div>
</template>
