<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import {
  api,
  isAdmin,
  libraries,
  loadAdminData,
  loadLibraries,
  logout,
  parentStack,
  scan,
  scanOperation,
  selectedItem,
  state,
  totalLibraryItems,
  user
} from '../store/app';
import CommandPalette from '../components/CommandPalette.vue';
import ContextMenu from '../components/ContextMenu.vue';
import type { ContextMenuItem } from '../components/ContextMenu.vue';
import MiniPlayer from '../components/MiniPlayer.vue';
import ShortcutsDialog from '../components/ShortcutsDialog.vue';
import { useAppToast } from '../composables/toast';

const toast = useAppToast();

const router = useRouter();
const route = useRoute();
const searchInput = ref('');
const paletteOpen = ref(false);
const shortcutsOpen = ref(false);
const locale = ref(localStorage.getItem('movie-rust-locale') || 'zh-CN');

const isAdminSection = computed(() => Boolean(route.meta.admin));
const isSettingsSection = computed(() => route.path.startsWith('/settings'));
const isHome = computed(() => route.path === '/');
const libraryRouteId = computed(() => String(route.params.id || ''));

const breadcrumb = computed(() => {
  const crumbs: Array<{ label: string; to?: string }> = [];
  if (route.name === 'home') {
    crumbs.push({ label: '首页' });
    return crumbs;
  }

  crumbs.push({ label: '首页', to: '/' });

  if (isSettingsSection.value) {
    crumbs.push({ label: '设置', to: route.path === '/settings' ? undefined : '/settings' });
    if (route.path !== '/settings' && route.meta.title) {
      crumbs.push({ label: String(route.meta.title) });
    }
    return crumbs;
  }

  if (route.name === 'library') {
    const lib = libraries.value.find((l) => l.Id === libraryRouteId.value);
    crumbs.push({ label: lib?.Name || '媒体库' });
    for (const parent of parentStack.value) {
      crumbs.push({ label: parent.Name });
    }
    return crumbs;
  }

  if (route.name === 'item' || route.name === 'series') {
    if (selectedItem.value?.Type === 'Episode' && selectedItem.value.SeriesName) {
      // Series 名做成可点击链接，回到 Series 详情页。
      crumbs.push({
        label: selectedItem.value.SeriesName,
        to: selectedItem.value.SeriesId
          ? `/series/${selectedItem.value.SeriesId}`
          : undefined
      });
    }
    if (selectedItem.value) {
      crumbs.push({ label: selectedItem.value.Name });
    } else {
      crumbs.push({ label: '详情' });
    }
    return crumbs;
  }

  if (route.meta.title) {
    crumbs.push({ label: String(route.meta.title) });
  }
  return crumbs;
});

const currentTitle = computed(() => breadcrumb.value[breadcrumb.value.length - 1]?.label || '首页');
const currentSubtitle = computed(() => {
  if (isAdminSection.value) return '服务器控制台';
  return user.value ? `欢迎回来，${user.value.Name}` : state.serverName;
});

const mainNavItems = computed(() => [
  { label: '首页', icon: 'i-lucide-home', to: '/', active: route.path === '/' },
  {
    label: '搜索',
    icon: 'i-lucide-search',
    to: '/search',
    active: route.path.startsWith('/search')
  },
  {
    label: '稍后观看',
    icon: 'i-lucide-clock',
    to: '/queue',
    active: route.path.startsWith('/queue')
  },
  {
    label: '播放列表',
    icon: 'i-lucide-list-music',
    to: '/playlists',
    active: route.path.startsWith('/playlist')
  },
  {
    label: '设置',
    icon: 'i-lucide-settings',
    to: '/settings',
    active: route.path.startsWith('/settings')
  }
]);

const libraryNavItems = computed(() =>
  libraries.value.map((library) => ({
    label: library.Name,
    icon: libraryIcon(library.CollectionType),
    to: `/library/${library.Id}`,
    active: route.path.startsWith(`/library/${library.Id}`),
    badge: library.ChildCount ? String(library.ChildCount) : undefined
  }))
);
const librarySummaryText = computed(() => {
  const count = totalLibraryItems.value;
  const operation = scanOperation.value;
  if (operation && !operation.Done) {
    const progress = Number.isFinite(operation.Progress) ? Math.round(operation.Progress) : 0;
    return `媒体库 · ${count} · ${operation.Status} ${progress}%`;
  }
  return `媒体库 · ${count}`;
});

const userAvatarSrc = computed(() => api.userImageUrl(user.value) || undefined);

const userMenuItems = computed(() => [
  [
    {
      label: user.value?.Name || '未登录',
      slot: 'account',
      disabled: true
    }
  ],
  [
    { label: '账户设置', icon: 'i-lucide-user', to: '/settings/account' },
    { label: '应用设置', icon: 'i-lucide-settings', to: '/settings' },
    {
      label: '键盘快捷键',
      icon: 'i-lucide-keyboard',
      onSelect: () => {
        shortcutsOpen.value = true;
      },
      kbd: ['?']
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

const localeOptions = [
  { label: '简体中文', value: 'zh-CN' },
  { label: 'English', value: 'en-US' }
];

function libraryIcon(collectionType?: string) {
  if (collectionType === 'movies') return 'i-lucide-clapperboard';
  if (collectionType === 'tvshows') return 'i-lucide-tv';
  return 'i-lucide-folder';
}

watch(
  () => route.query.q,
  (value) => {
    searchInput.value = typeof value === 'string' ? value : '';
  },
  { immediate: true }
);

// 监听是否进入/离开 admin 区块。只有"从非 admin 进入 admin"这一跳转时才加载一次，
// 避免在 /settings/a → /settings/b 之间反复拉 systemInfo / users / cultures / startup。
watch(
  () => isAdminSection.value,
  async (inAdmin, wasInAdmin) => {
    if (!inAdmin) return;
    if (!isAdmin.value) {
      await router.replace('/');
      return;
    }
    if (!wasInAdmin) {
      await loadAdminData();
    }
  }
);

watch(locale, (value) => {
  localStorage.setItem('movie-rust-locale', value);
  document.documentElement.lang = value;
});

onMounted(async () => {
  document.documentElement.lang = locale.value;
  window.addEventListener('keydown', onKeyDown);
  try {
    if (!libraries.value.length) await loadLibraries();
    if (isAdminSection.value) {
      if (!isAdmin.value) {
        await router.replace('/');
        return;
      }
      await loadAdminData();
    }
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
  }
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeyDown);
});

async function submitSearch() {
  const query = searchInput.value.trim();
  if (!query) {
    if (route.name === 'search') await router.replace('/');
    return;
  }
  await router.push({ name: 'search', query: { q: query } });
}

let searchTimer = 0;
function onSearchInput() {
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(() => {
    const query = searchInput.value.trim();
    if (!query) {
      if (route.name === 'search') void router.replace('/');
      return;
    }
    if (route.name === 'search' && route.query.q === query) return;
    void router.replace({ name: 'search', query: { q: query } });
  }, 350);
}

async function handleLogout() {
  logout();
  await router.replace('/server/login');
}

let gPressed = false;
let gTimer = 0;
function onKeyDown(e: KeyboardEvent) {
  const target = e.target as HTMLElement | null;
  const inInput =
    target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA' || target?.isContentEditable;

  if ((e.metaKey || e.ctrlKey) && (e.key === 'k' || e.key === 'K')) {
    e.preventDefault();
    paletteOpen.value = !paletteOpen.value;
    return;
  }
  if (inInput) return;
  if (e.key === '/') {
    e.preventDefault();
    paletteOpen.value = true;
    return;
  }
  if (e.key === '?') {
    e.preventDefault();
    shortcutsOpen.value = true;
    return;
  }
  if (e.key === 'g' || e.key === 'G') {
    gPressed = true;
    window.clearTimeout(gTimer);
    gTimer = window.setTimeout(() => {
      gPressed = false;
    }, 800);
    return;
  }
  if (gPressed && (e.key === 'h' || e.key === 'H')) {
    gPressed = false;
    void router.push('/');
    return;
  }
  if (gPressed && (e.key === 's' || e.key === 'S')) {
    gPressed = false;
    void router.push('/settings');
  }
}

const sidebarCtxMenu = ref<InstanceType<typeof ContextMenu> | null>(null);
const sidebarCtxLibrary = ref<{ id: string; name: string } | null>(null);

function openSidebarLibraryCtx(e: MouseEvent, libId: string, libName: string) {
  sidebarCtxLibrary.value = { id: libId, name: libName };
  sidebarCtxMenu.value?.show(e);
}

const sidebarLibMenuItems = computed<ContextMenuItem[][]>(() => {
  const lib = sidebarCtxLibrary.value;
  if (!lib) return [];
  const items: ContextMenuItem[][] = [
    [
      {
        label: '打开媒体库',
        icon: 'i-lucide-folder-open',
        onSelect: () => router.push(`/library/${lib.id}`)
      }
    ]
  ];
  if (isAdmin.value) {
    items.push([
      {
        label: '扫描媒体库',
        icon: 'i-lucide-refresh-ccw',
        onSelect: async () => {
          try {
            await scan(lib.id);
            toast.success(`正在扫描 "${lib.name}"`);
          } catch {
            toast.error('扫描失败');
          }
        }
      },
      {
        label: '刷新元数据',
        icon: 'i-lucide-refresh-cw',
        onSelect: async () => {
          try {
            await api.refreshItemMetadata(lib.id);
            toast.success('元数据刷新已提交');
          } catch {
            toast.error('刷新元数据失败');
          }
        }
      },
      {
        label: '管理媒体库',
        icon: 'i-lucide-settings',
        onSelect: () => router.push('/settings/libraries')
      }
    ]);
  }
  return items;
});
</script>

<template>
  <UDashboardGroup unit="rem" storage="local" class="min-h-svh min-w-0">
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
            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-primary text-sm font-bold text-primary-contrast"
          >
            MR
          </div>
          <div v-if="!collapsed" class="min-w-0">
            <p class="text-muted text-[10px] uppercase tracking-wider">Movie Rust</p>
            <p class="text-highlighted display-font truncate text-base font-semibold">
              {{ state.serverName }}
            </p>
          </div>
        </RouterLink>

        <UColorModeButton v-if="!collapsed" class="ms-auto" size="xs" />
      </template>

      <template #default="{ collapsed }">
        <UNavigationMenu :collapsed="collapsed" :items="mainNavItems" orientation="vertical" />

        <USeparator v-if="libraryNavItems.length" type="dashed" />

        <div v-if="!collapsed && libraryNavItems.length" class="px-2 py-1">
          <p class="text-muted text-[10px] font-medium uppercase tracking-wider">
            {{ librarySummaryText }}
          </p>
        </div>
        <nav v-if="libraryNavItems.length && !collapsed" class="flex flex-col gap-0.5 px-2">
          <RouterLink
            v-for="navItem in libraryNavItems"
            :key="navItem.to"
            :to="navItem.to"
            class="group flex items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors"
            :class="navItem.active ? 'bg-primary/10 text-primary font-medium' : 'text-muted hover:bg-default hover:text-highlighted'"
            @contextmenu="openSidebarLibraryCtx($event, navItem.to.replace('/library/', ''), navItem.label)"
          >
            <UIcon :name="navItem.icon" class="size-4 shrink-0" />
            <span class="truncate flex-1">{{ navItem.label }}</span>
            <span v-if="navItem.badge" class="text-muted text-xs tabular-nums">{{ navItem.badge }}</span>
          </RouterLink>
        </nav>
        <UNavigationMenu
          v-else-if="libraryNavItems.length && collapsed"
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
            <UAvatar size="xs" :alt="user?.Name || 'U'" :src="userAvatarSrc" />
            <span v-if="!collapsed" class="truncate text-sm">
              {{ user?.Name || '未登录' }}
            </span>
            <UIcon
              v-if="!collapsed"
              name="i-lucide-chevrons-up-down"
              class="text-dimmed ms-auto size-4"
            />
          </UButton>
        </UDropdownMenu>
      </template>
    </UDashboardSidebar>

    <UDashboardPanel id="main" class="flex min-h-0 min-w-0 flex-1 flex-col">
      <UDashboardNavbar :title="currentTitle" :toggle="{ color: 'neutral', variant: 'ghost' }">
        <template #leading>
          <UDashboardSidebarCollapse />
        </template>

        <template #title>
          <div class="flex flex-col leading-tight">
            <span class="text-muted text-xs">{{ currentSubtitle }}</span>
            <div class="flex items-center gap-1">
              <template v-for="(crumb, idx) in breadcrumb" :key="idx">
                <button
                  v-if="crumb.to"
                  type="button"
                  class="text-muted hover:text-primary text-base font-semibold"
                  @click="router.push(crumb.to)"
                >
                  {{ crumb.label }}
                </button>
                <span v-else class="text-highlighted display-font truncate text-base font-semibold">
                  {{ crumb.label }}
                </span>
                <UIcon
                  v-if="idx < breadcrumb.length - 1"
                  name="i-lucide-chevron-right"
                  class="text-muted size-4"
                />
              </template>
            </div>
          </div>
        </template>

        <template #right>
          <UButton
            class="hidden md:inline-flex"
            color="neutral"
            variant="outline"
            size="sm"
            @click="paletteOpen = true"
          >
            <UIcon name="i-lucide-search" class="size-4" />
            <span class="text-muted">快速搜索</span>
            <span class="text-muted ms-3 hidden items-center gap-0.5 lg:flex">
              <UKbd>⌘</UKbd>
              <UKbd>K</UKbd>
            </span>
          </UButton>

          <form v-if="!isAdminSection" class="hidden md:block" @submit.prevent="submitSearch">
            <UInput
              v-model="searchInput"
              icon="i-lucide-search"
              placeholder="搜索"
              class="w-56"
              @update:model-value="onSearchInput"
            />
          </form>

          <USelect
            v-model="locale"
            :items="localeOptions"
            value-key="value"
            class="hidden w-28 md:block"
            size="sm"
          />

          <UButton
            icon="i-lucide-keyboard"
            color="neutral"
            variant="ghost"
            aria-label="键盘快捷键"
            @click="shortcutsOpen = true"
          />

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

      <div class="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto p-4 sm:p-6">
        <slot />
      </div>
    </UDashboardPanel>

    <CommandPalette v-model:open="paletteOpen" />
    <ShortcutsDialog v-model:open="shortcutsOpen" />
    <MiniPlayer />

    <ContextMenu
      ref="sidebarCtxMenu"
      :items="sidebarLibMenuItems"
      :preview-title="sidebarCtxLibrary?.name"
    />
  </UDashboardGroup>
</template>
