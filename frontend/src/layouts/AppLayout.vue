<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import {
  isAdmin,
  libraries,
  loadAdminData,
  loadLibraries,
  logout,
  state,
  user
} from '../store/app';

const router = useRouter();
const route = useRoute();
const searchInput = ref('');

const isAdminSection = computed(() => Boolean(route.meta.admin));
const isHome = computed(() => route.path === '/');
const libraryRouteId = computed(() => String(route.params.id || ''));
const currentTitle = computed(() => {
  if (route.name === 'library') {
    return libraries.value.find((library) => library.Id === libraryRouteId.value)?.Name || '媒体库';
  }

  return String(route.meta.title || '首页');
});
const currentSubtitle = computed(() => {
  if (isAdminSection.value) {
    return '服务器控制台';
  }

  return user.value ? `欢迎回来，${user.value.Name}` : state.serverName;
});

const mainNavItems = computed(() => {
  const items = [
    {
      label: '首页',
      icon: 'i-lucide-home',
      to: '/',
      active: route.path === '/'
    },
    {
      label: '搜索',
      icon: 'i-lucide-search',
      to: '/search',
      active: route.path.startsWith('/search')
    },
    {
      label: '设置',
      icon: 'i-lucide-settings',
      to: '/settings',
      active: route.path.startsWith('/settings')
    }
  ];

  return items;
});

const libraryNavItems = computed(() =>
  libraries.value.map((library) => ({
    label: library.Name,
    icon: libraryIcon(library.CollectionType),
    to: `/library/${library.Id}`,
    active: route.path.startsWith(`/library/${library.Id}`),
    badge: library.ChildCount ? String(library.ChildCount) : undefined
  }))
);

const userMenuItems = computed(() => [
  [
    {
      label: user.value?.Name || '未登录',
      slot: 'account',
      disabled: true
    }
  ],
  [
    {
      label: '账户设置',
      icon: 'i-lucide-user',
      to: '/settings/account'
    },
    {
      label: '应用设置',
      icon: 'i-lucide-settings',
      to: '/settings'
    }
  ],
  [
    {
      label: '退出登录',
      icon: 'i-lucide-log-out',
      color: 'error' as const,
      onSelect: handleLogout
    }
  ]
]);

function libraryIcon(collectionType?: string) {
  if (collectionType === 'movies') return 'i-lucide-clapperboard';
  if (collectionType === 'tvshows') return 'i-lucide-tv';
  if (collectionType === 'music') return 'i-lucide-music';
  if (collectionType === 'books') return 'i-lucide-book';
  return 'i-lucide-folder';
}

watch(
  () => route.query.q,
  (value) => {
    searchInput.value = typeof value === 'string' ? value : '';
  },
  { immediate: true }
);

watch(
  () => route.fullPath,
  async () => {
    if (!isAdminSection.value) {
      return;
    }

    if (!isAdmin.value) {
      await router.replace('/');
      return;
    }

    await loadAdminData();
  }
);

onMounted(async () => {
  if (!libraries.value.length) {
    await loadLibraries();
  }

  if (isAdminSection.value) {
    if (!isAdmin.value) {
      await router.replace('/');
      return;
    }

    await loadAdminData();
  }
});

async function submitSearch() {
  const query = searchInput.value.trim();
  if (!query) {
    if (route.name === 'search') {
      await router.replace('/');
    }
    return;
  }

  await router.push({ name: 'search', query: { q: query } });
}

// 输入时进行节流式实时跳转到搜索页，减少 Enter 才能看结果的摩擦。
let searchTimer = 0;
function onSearchInput() {
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(() => {
    const query = searchInput.value.trim();
    if (!query) {
      if (route.name === 'search') {
        void router.replace('/');
      }
      return;
    }
    if (route.name === 'search' && route.query.q === query) {
      return;
    }
    void router.replace({ name: 'search', query: { q: query } });
  }, 350);
}

async function handleLogout() {
  logout();
  await router.replace('/server/login');
}
</script>

<template>
  <UDashboardGroup unit="rem" storage="local">
    <UDashboardSidebar
      class="bg-elevated/25"
      resizable
      collapsible
      :toggle="{ size: 'sm', variant: 'outline', class: 'ring-default' }"
      :ui="{ footer: 'border-t border-default' }"
    >
      <template #header="{ collapsed }">
        <RouterLink to="/" class="flex items-center gap-3">
          <div
            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-primary text-primary-contrast text-sm font-bold"
          >
            MR
          </div>
          <div v-if="!collapsed" class="min-w-0">
            <p class="text-muted text-[10px] uppercase tracking-wider">Movie Rust</p>
            <p class="text-highlighted truncate text-sm font-semibold">
              {{ state.serverName }}
            </p>
          </div>
        </RouterLink>

        <UColorModeButton v-if="!collapsed" class="ms-auto" size="xs" />
      </template>

      <template #default="{ collapsed }">
        <UNavigationMenu
          :collapsed="collapsed"
          :items="mainNavItems"
          orientation="vertical"
        />

        <USeparator v-if="libraryNavItems.length" type="dashed" />

        <div v-if="!collapsed && libraryNavItems.length" class="px-2 py-1">
          <p class="text-muted text-[10px] font-medium uppercase tracking-wider">
            媒体库
          </p>
        </div>
        <UNavigationMenu
          v-if="libraryNavItems.length"
          :collapsed="collapsed"
          :items="libraryNavItems"
          orientation="vertical"
        />
      </template>

      <template #footer="{ collapsed }">
        <UDropdownMenu
          :items="userMenuItems"
          :content="{ align: 'start', side: 'top' }"
          :ui="{ content: 'w-56' }"
        >
          <UButton
            :variant="collapsed ? 'ghost' : 'soft'"
            color="neutral"
            :block="!collapsed"
            class="justify-start"
          >
            <UAvatar size="xs" :alt="user?.Name || 'U'" />
            <span v-if="!collapsed" class="truncate text-sm">
              {{ user?.Name || '未登录' }}
            </span>
            <UIcon
              v-if="!collapsed"
              name="i-lucide-chevrons-up-down"
              class="ms-auto size-4 text-dimmed"
            />
          </UButton>
        </UDropdownMenu>
      </template>
    </UDashboardSidebar>

    <UDashboardPanel id="main">
      <UDashboardNavbar :title="currentTitle" :toggle="{ color: 'neutral', variant: 'ghost' }">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>

        <template #title>
          <div class="flex flex-col leading-tight">
            <span class="text-muted text-xs">{{ currentSubtitle }}</span>
            <span class="text-highlighted text-base font-semibold">{{ currentTitle }}</span>
          </div>
        </template>

        <template #right>
          <form
            v-if="!isAdminSection"
            class="hidden md:block"
            @submit.prevent="submitSearch"
          >
            <UInput
              v-model="searchInput"
              icon="i-lucide-search"
              placeholder="搜索电影、剧集"
              class="w-64"
              @update:model-value="onSearchInput"
            />
          </form>
          <UButton
            v-if="!isHome"
            icon="i-lucide-arrow-left"
            color="neutral"
            variant="ghost"
            aria-label="返回"
            @click="router.back()"
          />
        </template>
      </UDashboardNavbar>

      <UAlert
        v-if="state.error"
        icon="i-lucide-triangle-alert"
        color="error"
        variant="subtle"
        :title="state.error"
        close
        class="mx-4 mt-3"
        @update:model-value="state.error = ''"
      />
      <UAlert
        v-else-if="state.message"
        icon="i-lucide-info"
        color="primary"
        variant="subtle"
        :title="state.message"
        close
        class="mx-4 mt-3"
        @update:model-value="state.message = ''"
      />

      <template #body>
        <div class="flex flex-col gap-6 p-4 sm:p-6">
          <slot />
        </div>
      </template>
    </UDashboardPanel>
  </UDashboardGroup>
</template>
