<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import { api } from '../../store/app';

const router = useRouter();
const step = ref<'request' | 'pin'>('request');

const username = ref('');
const pin = ref('');
const newPassword = ref('');
const confirmPassword = ref('');
const expiresAt = ref<string | null>(null);

const loading = ref(false);
const error = ref('');
const saved = ref('');

async function requestPin() {
  if (!username.value.trim()) {
    error.value = '请输入用户名';
    return;
  }
  loading.value = true;
  error.value = '';
  saved.value = '';
  try {
    const result = await api.forgotPassword(username.value.trim());
    expiresAt.value = result.PinExpirationDate || null;
    step.value = 'pin';
    saved.value = '已生成 PIN，请查询服务端日志或联系管理员获取，并在下方输入。';
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}

async function confirmPin() {
  if (!pin.value.trim()) {
    error.value = '请输入 PIN';
    return;
  }
  if (newPassword.value.length < 4) {
    error.value = '新密码至少需要 4 个字符';
    return;
  }
  if (newPassword.value !== confirmPassword.value) {
    error.value = '两次输入的新密码不一致';
    return;
  }
  loading.value = true;
  error.value = '';
  try {
    const result = await api.forgotPasswordPin(pin.value.trim(), newPassword.value);
    if (result.Success) {
      saved.value = '密码已重置，正在跳转登录页...';
      setTimeout(() => router.replace('/server/login'), 1500);
    } else {
      error.value = 'PIN 无效或已过期';
    }
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}
</script>

<template>
  <div class="mx-auto flex min-h-svh w-full max-w-md flex-col items-center justify-center gap-6 p-6">
    <div class="flex flex-col items-center gap-2">
      <div class="bg-primary text-primary-contrast flex size-12 items-center justify-center rounded-xl text-base font-bold">
        MR
      </div>
      <h1 class="text-highlighted display-font text-2xl font-semibold">找回密码</h1>
      <p class="text-muted text-xs">请按步骤验证身份并重置密码。</p>
    </div>

    <UCard class="w-full">
      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
      <UAlert v-else-if="saved" color="success" icon="i-lucide-check" :description="saved" />

      <form v-if="step === 'request'" class="space-y-4" @submit.prevent="requestPin">
        <UFormField label="用户名">
          <UInput v-model="username" autocomplete="username" class="w-full" />
        </UFormField>
        <p class="text-muted text-xs">
          提交后，服务器将为该用户生成一个 30 分钟有效的一次性 PIN，并记录在服务器日志中。
        </p>
        <div class="flex items-center justify-between">
          <UButton variant="subtle" color="neutral" @click="router.push('/server/login')">
            返回登录
          </UButton>
          <UButton type="submit" :loading="loading" icon="i-lucide-mail">获取 PIN</UButton>
        </div>
      </form>

      <form v-else class="space-y-4" @submit.prevent="confirmPin">
        <UFormField label="PIN" hint="6 位数字">
          <UInput v-model="pin" maxlength="6" inputmode="numeric" class="w-full" />
        </UFormField>
        <UFormField label="新密码">
          <UInput v-model="newPassword" type="password" autocomplete="new-password" class="w-full" />
        </UFormField>
        <UFormField label="确认新密码">
          <UInput v-model="confirmPassword" type="password" autocomplete="new-password" class="w-full" />
        </UFormField>
        <p v-if="expiresAt" class="text-muted text-xs">
          PIN 将在 {{ new Date(expiresAt).toLocaleString() }} 过期。
        </p>
        <div class="flex items-center justify-between">
          <UButton variant="subtle" color="neutral" @click="step = 'request'">
            重新发送
          </UButton>
          <UButton type="submit" :loading="loading" icon="i-lucide-key">重置密码</UButton>
        </div>
      </form>
    </UCard>
  </div>
</template>
