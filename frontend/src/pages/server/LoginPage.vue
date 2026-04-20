<script setup lang="ts">
import { useRouter } from 'vue-router';
import { currentServer, login, publicUsers, state, user } from '../../store/app';

const router = useRouter();

async function submitLogin(name = state.username, password = state.password) {
  await login(name, password);
  if (user.value) {
    await router.replace('/');
  }
}
</script>

<template>
  <section class="server-screen">
    <div class="server-card">
      <div class="server-brand centered">
        <div class="mark">MR</div>
        <h1>{{ state.serverName }}</h1>
        <p>{{ currentServer?.Url || '当前服务器' }}</p>
      </div>

      <div v-if="publicUsers.length && !state.loginAsOther" class="user-picker">
        <h2>选择用户</h2>
        <div class="user-grid">
          <button
            v-for="publicUser in publicUsers"
            :key="publicUser.Id"
            type="button"
            @click="state.username = publicUser.Name; state.loginAsOther = true"
          >
            <span>{{ publicUser.Name.slice(0, 1).toUpperCase() }}</span>
            {{ publicUser.Name }}
          </button>
        </div>
        <div class="button-row">
          <button class="secondary" type="button" @click="state.loginAsOther = true">手动登录</button>
          <button class="secondary" type="button" @click="router.push('/server/select')">切换服务器</button>
          <button class="secondary" type="button" @click="router.push('/server/add')">添加服务器</button>
        </div>
      </div>

      <form v-else class="form-stack" @submit.prevent="submitLogin()">
        <h2>登录</h2>
        <label>
          用户名
          <input v-model="state.username" autocomplete="username" />
        </label>
        <label>
          密码
          <div class="password-field">
            <input
              v-model="state.password"
              :type="state.showLoginPassword ? 'text' : 'password'"
              autocomplete="current-password"
            />
            <button
              type="button"
              :title="state.showLoginPassword ? '隐藏密码' : '显示密码'"
              @click="state.showLoginPassword = !state.showLoginPassword"
            >
              {{ state.showLoginPassword ? '隐藏' : '显示' }}
            </button>
          </div>
        </label>
        <div class="button-row">
          <button v-if="publicUsers.length" class="secondary" type="button" @click="state.loginAsOther = false">
            返回
          </button>
          <button class="secondary" type="button" @click="router.push('/server/select')">服务器</button>
          <button :disabled="state.busy" type="submit">登录</button>
        </div>
      </form>

      <p v-if="state.error" class="notice error">{{ state.error }}</p>
    </div>
  </section>
</template>
