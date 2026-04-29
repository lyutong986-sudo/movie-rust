<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../../components/MediaCard.vue';
import type { BaseItemDto } from '../../api/emby';
import { api } from '../../store/app';
import { itemRoute, playbackRoute } from '../../utils/navigation';

const route = useRoute();
const router = useRouter();

const loading = ref(true);
const error = ref('');
const person = ref<BaseItemDto | null>(null);
const items = ref<BaseItemDto[]>([]);

const personId = computed(() => String(route.params.id || ''));

const personImageUrl = computed(() => {
  if (!person.value) return '';
  return api.personImageUrl(person.value);
});

const externalLinks = computed(() => {
  const links: { name: string; url: string; icon: string }[] = [];
  const ids = person.value?.ProviderIds;
  if (!ids) return links;
  if (ids.Tmdb) links.push({ name: 'TMDB', url: `https://www.themoviedb.org/person/${ids.Tmdb}`, icon: 'i-lucide-film' });
  if (ids.Imdb) links.push({ name: 'IMDb', url: `https://www.imdb.com/name/${ids.Imdb}`, icon: 'i-lucide-star' });
  if (ids.Douban) links.push({ name: '豆瓣', url: `https://movie.douban.com/celebrity/${ids.Douban}/`, icon: 'i-lucide-book-open' });
  return links;
});

const birthYear = computed(() => person.value?.ProductionYear);

watch(
  () => route.params.id,
  async () => {
    if (!personId.value) return;
    await loadPerson();
  },
  { immediate: true }
);

async function loadPerson() {
  loading.value = true;
  error.value = '';
  person.value = null;
  items.value = [];

  try {
    const [personData, personItems] = await Promise.all([
      api.getPerson(personId.value),
      api.getPersonItems(personId.value, { limit: 200 })
    ]);
    person.value = personData;
    items.value = personItems;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

async function openItem(item: BaseItemDto) {
  await router.push(itemRoute(item));
}

async function playItem(item: BaseItemDto) {
  await router.push(playbackRoute(item));
}
</script>

<template>
  <div class="flex flex-col gap-6">
    <nav class="flex items-center gap-2 text-sm">
      <UButton
        color="neutral"
        variant="ghost"
        size="xs"
        icon="i-lucide-arrow-left"
        @click="router.back()"
      >
        返回
      </UButton>
      <UIcon name="i-lucide-chevron-right" class="size-3 text-muted" />
      <span class="text-muted">演员</span>
      <UIcon name="i-lucide-chevron-right" class="size-3 text-muted" />
      <span class="text-highlighted font-medium">{{ person?.Name || '加载中…' }}</span>
    </nav>

    <div v-if="loading" class="flex min-h-[40vh] flex-col items-center justify-center gap-2">
      <UProgress animation="carousel" class="w-48" />
      <p class="text-muted text-sm">正在加载演员信息…</p>
    </div>

    <UAlert
      v-else-if="error"
      color="error"
      variant="subtle"
      icon="i-lucide-triangle-alert"
      title="加载失败"
      :description="error"
    />

    <template v-else-if="person">
      <div class="flex flex-col gap-6 sm:flex-row">
        <div class="shrink-0">
          <div class="relative mx-auto w-40 overflow-hidden rounded-xl border border-default shadow-lg sm:mx-0 sm:w-48">
            <img
              v-if="personImageUrl"
              :src="personImageUrl"
              :alt="person.Name"
              class="aspect-[2/3] w-full object-cover"
              loading="lazy"
            />
            <div
              v-else
              class="flex aspect-[2/3] w-full items-center justify-center bg-elevated/40 text-4xl font-bold text-muted"
            >
              {{ (person.Name || '?').slice(0, 1).toUpperCase() }}
            </div>
          </div>
        </div>

        <div class="flex min-w-0 flex-1 flex-col gap-4">
          <div>
            <h1 class="text-highlighted text-2xl font-bold sm:text-3xl">{{ person.Name }}</h1>
            <div class="mt-1 flex flex-wrap items-center gap-2 text-sm text-muted">
              <span v-if="birthYear">出生年份: {{ birthYear }}</span>
              <span v-if="items.length">· {{ items.length }} 部作品</span>
            </div>
          </div>

          <p
            v-if="person.Overview"
            class="text-dimmed max-w-3xl text-sm leading-relaxed"
          >
            {{ person.Overview }}
          </p>

          <div v-if="externalLinks.length" class="flex flex-wrap gap-2">
            <a
              v-for="link in externalLinks"
              :key="link.name"
              :href="link.url"
              target="_blank"
              rel="noopener noreferrer"
              class="inline-flex items-center gap-1.5 rounded-lg border border-default bg-elevated/30 px-3 py-1.5 text-sm text-highlighted transition hover:bg-elevated/60 hover:text-primary"
            >
              <UIcon :name="link.icon" class="size-4" />
              {{ link.name }}
              <UIcon name="i-lucide-external-link" class="size-3 text-muted" />
            </a>
          </div>
        </div>
      </div>

      <section v-if="items.length" class="space-y-4">
        <div class="flex items-center gap-2">
          <UIcon name="i-lucide-clapperboard" class="size-5 text-primary" />
          <h2 class="text-highlighted text-lg font-semibold">参演作品</h2>
          <span class="text-muted text-sm">({{ items.length }})</span>
        </div>
        <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 xl:grid-cols-7">
          <MediaCard
            v-for="item in items"
            :key="item.Id"
            :item="item"
            @play="playItem"
            @select="openItem"
          />
        </div>
      </section>

      <div
        v-else
        class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default bg-elevated/20 p-10 text-center"
      >
        <UIcon name="i-lucide-user" class="size-10 text-muted" />
        <h3 class="text-highlighted text-lg font-semibold">暂无相关作品</h3>
        <p class="text-muted max-w-md text-sm">
          当前媒体库中没有找到与该演员关联的作品。
        </p>
      </div>
    </template>
  </div>
</template>
