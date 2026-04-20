<script setup lang="ts">
import { createLibrary, state } from '../store/app';

const emit = defineEmits<{
  close: [];
}>();

async function submit() {
  await createLibrary();
  if (!state.error) {
    emit('close');
  }
}
</script>

<template>
  <div class="dialog-backdrop" @click.self="emit('close')">
    <section class="small-dialog">
      <button class="close" type="button" aria-label="关闭" @click="emit('close')">×</button>
      <form class="form-stack settings-form" @submit.prevent="submit">
        <h2>添加媒体库</h2>
        <label>
          媒体库名称
          <input v-model="state.libraryName" placeholder="例如：电影" />
        </label>
        <label>
          媒体类型
          <select v-model="state.collectionType">
            <option value="movies">电影</option>
            <option value="tvshows">电视剧</option>
            <option value="music">音乐</option>
            <option value="homevideos">家庭视频</option>
          </select>
        </label>
        <label>
          文件路径
          <input v-model="state.libraryPath" placeholder="/media/movies" />
        </label>
        <div class="button-row">
          <button class="secondary" type="button" @click="emit('close')">取消</button>
          <button :disabled="state.busy" type="submit">创建</button>
        </div>
      </form>
    </section>
  </div>
</template>
