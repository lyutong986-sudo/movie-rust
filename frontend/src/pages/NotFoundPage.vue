<script setup lang="ts">
import { useRouter } from 'vue-router';

const router = useRouter();
</script>

<template>
  <div class="flex min-h-[70vh] flex-col items-center justify-center gap-6 text-center">
    <div class="display-font text-primary text-[120px] font-bold leading-none">404</div>
    <div class="space-y-2">
      <h1 class="text-highlighted text-2xl font-semibold">找不到这个页面</h1>
      <p class="text-muted max-w-md text-sm">
        你访问的链接可能已过期、被删除或拼写错误。你可以回到首页或搜索想看的内容。
      </p>
    </div>
    <div class="flex gap-2">
      <UButton icon="i-lucide-home" @click="router.push('/')">返回首页</UButton>
      <UButton color="neutral" variant="soft" icon="i-lucide-search" @click="router.push('/search')">
        搜索内容
      </UButton>
    </div>
  </div>
</template>
