<script setup lang="ts">
import SettingsLayout from '../../layouts/SettingsLayout.vue';
</script>

<template>
  <SettingsLayout>
    <div class="space-y-4">
      <div class="grid gap-3 sm:grid-cols-3">
        <UCard variant="soft">
          <p class="text-muted text-xs">认证方式</p>
          <p class="text-highlighted mt-1 text-base font-semibold">X-Emby-Authorization</p>
          <p class="text-muted text-xs">兼容 Emby 客户端登录头</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">播放模式</p>
          <p class="text-highlighted mt-1 text-base font-semibold">Direct Stream</p>
          <p class="text-muted text-xs">优先直链，保留 Emby/本地播放器接入</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">字幕来源</p>
          <p class="text-highlighted mt-1 text-base font-semibold">外挂字幕</p>
          <p class="text-muted text-xs">自动识别 sidecar subtitle 文件</p>
        </UCard>
      </div>

      <div class="grid gap-3 md:grid-cols-3">
        <UCard>
          <div class="flex items-start gap-3">
            <UIcon name="i-lucide-monitor-play" class="text-primary size-5" />
            <div>
              <h4 class="text-highlighted text-sm font-semibold">本地播放器联动</h4>
              <p class="text-muted mt-1 text-xs">
                服务端已兼容 Emby 风格登录和播放流地址，适合直接接入你现有的本地播放器。
              </p>
            </div>
          </div>
        </UCard>
        <UCard>
          <div class="flex items-start gap-3">
            <UIcon name="i-lucide-activity" class="text-primary size-5" />
            <div>
              <h4 class="text-highlighted text-sm font-semibold">播放进度回传</h4>
              <p class="text-muted mt-1 text-xs">
                已支持 Favorite、Played、UserData 更新接口，可继续扩展进度同步和会话控制。
              </p>
            </div>
          </div>
        </UCard>
        <UCard>
          <div class="flex items-start gap-3">
            <UIcon name="i-lucide-audio-lines" class="text-primary size-5" />
            <div>
              <h4 class="text-highlighted text-sm font-semibold">详情页轨道选择</h4>
              <p class="text-muted mt-1 text-xs">
                详情页已能读取媒体源、音轨、字幕轨信息，后续可以继续对齐 Jellyfin 的播放器页。
              </p>
            </div>
          </div>
        </UCard>
      </div>
    </div>
  </SettingsLayout>
</template>
