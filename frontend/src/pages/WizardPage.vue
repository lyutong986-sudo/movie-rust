<script setup lang="ts">
import {
  completeWizard,
  createWizardAdmin,
  saveLanguageAndContinue,
  saveMetadataAndContinue,
  state
} from '../store/app';
</script>

<template>
  <section class="server-screen">
    <div class="server-card wizard-card">
      <div class="server-brand">
        <div class="mark">MR</div>
        <div>
          <p>Movie Rust</p>
          <h1>欢迎使用 {{ state.serverName }}</h1>
        </div>
      </div>
      <div class="steps" aria-label="首次启动向导">
        <span :class="{ active: state.wizardStep === 1, done: state.wizardStep > 1 }">语言</span>
        <span :class="{ active: state.wizardStep === 2, done: state.wizardStep > 2 }">管理员</span>
        <span :class="{ active: state.wizardStep === 3, done: state.wizardStep > 3 }">元数据</span>
        <span :class="{ active: state.wizardStep === 4 }">远程访问</span>
      </div>

      <div v-if="state.wizardStep === 1" class="wizard-pane">
        <h2>选择你的媒体服务器语言</h2>
        <p>当前界面使用简体中文，后端会按 Jellyfin/Emby 的 Startup 配置接口保存首选语言。</p>
        <label>
          显示语言
          <select v-model="state.uiCulture">
            <option value="zh-CN">简体中文</option>
            <option value="en-US">English</option>
          </select>
        </label>
        <button :disabled="state.busy" type="button" @click="saveLanguageAndContinue">继续</button>
      </div>

      <form v-else-if="state.wizardStep === 2" class="wizard-pane form-stack" @submit.prevent="createWizardAdmin">
        <h2>创建管理员账户</h2>
        <p v-if="state.adminCreated">管理员账户已经创建，可以继续设置元数据。</p>
        <label>
          用户名
          <input v-model="state.adminName" autocomplete="username" :disabled="state.adminCreated" />
        </label>
        <label>
          密码
          <div class="password-field">
            <input
              v-model="state.adminPassword"
              :type="state.showWizardPassword ? 'text' : 'password'"
              autocomplete="new-password"
              :disabled="state.adminCreated"
            />
            <button
              type="button"
              :title="state.showWizardPassword ? '隐藏密码' : '显示密码'"
              @click="state.showWizardPassword = !state.showWizardPassword"
            >
              {{ state.showWizardPassword ? '隐藏' : '显示' }}
            </button>
          </div>
        </label>
        <label>
          确认密码
          <input
            v-model="state.adminPasswordConfirm"
            :type="state.showWizardPassword ? 'text' : 'password'"
            autocomplete="new-password"
            :disabled="state.adminCreated"
          />
        </label>
        <div class="button-row">
          <button class="secondary" type="button" @click="state.wizardStep = 1">返回</button>
          <button :disabled="state.busy" type="submit">继续</button>
        </div>
      </form>

      <form v-else-if="state.wizardStep === 3" class="wizard-pane form-stack" @submit.prevent="saveMetadataAndContinue">
        <h2>首选元数据语言</h2>
        <p>这一项对应 Jellyfin 的元数据语言配置，后续扫描和识别媒体时会沿用这些首选项。</p>
        <label>
          元数据语言
          <select v-model="state.metadataLanguage">
            <option value="zh">中文</option>
            <option value="en">English</option>
            <option value="ja">日本語</option>
            <option value="ko">한국어</option>
          </select>
        </label>
        <label>
          元数据国家/地区
          <select v-model="state.metadataCountry">
            <option value="CN">中国</option>
            <option value="US">United States</option>
            <option value="JP">日本</option>
            <option value="KR">韩国</option>
          </select>
        </label>
        <div class="button-row">
          <button class="secondary" type="button" @click="state.wizardStep = 2">返回</button>
          <button :disabled="state.busy" type="submit">继续</button>
        </div>
      </form>

      <div v-else class="wizard-pane">
        <h2>远程访问</h2>
        <p>保留 Jellyfin/Emby 的远程访问配置入口。当前版本会保存选择，实际端口映射可以在部署层继续配置。</p>
        <label class="check-row">
          <input v-model="state.allowRemoteAccess" type="checkbox" />
          允许远程连接到服务器
        </label>
        <label class="check-row">
          <input v-model="state.enableUPNP" type="checkbox" />
          自动端口映射
        </label>
        <div class="button-row">
          <button class="secondary" type="button" @click="state.wizardStep = 3">返回</button>
          <button :disabled="state.busy" type="button" @click="completeWizard">完成设置</button>
        </div>
      </div>

      <p v-if="state.error" class="notice error">{{ state.error }}</p>
    </div>
  </section>
</template>
