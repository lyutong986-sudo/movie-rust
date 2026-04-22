<template>
  <SettingsPage>
    <template #title>
      {{ t('users') }}
    </template>
    <template #actions>
      <VBtn
        variant="elevated"
        href="https://jellyfin.org/docs/general/server/users/"
        rel="noreferrer noopener"
        target="_blank">
        {{ t('help') }}
      </VBtn>
    </template>
    <template #content>
      <VCard
        width="100%"
        height="100%">
        <VTabs
          v-model="tab"
          color="deep-purple-accent-4"
          align-tabs="center">
          <VTab :value="1">
            {{ t("profile") }}
          </VTab>
          <VTab :value="2">
            {{ t("access") }}
          </VTab>
          <VTab :value="3">
            {{ t("parentalControl") }}
          </VTab>
          <VTab :value="4">
            {{ t("password") }}
          </VTab>
        </VTabs>
        <VWindow v-model="tab">
          <VWindowItem
            :key="1"
            :value="1">
            <VForm>
              <VContainer>
                <VRow>
                  <VCol>
                    <VTextField
                      v-model="model.Name"
                      :label="t('name')"
                      hide-details />
                  </VCol>
                </VRow>
                <VRow>
                  <VCol
                    cols="12"
                    md="6">
                    <VCheckbox
                      v-model="model.IsAdministrator"
                      :label="t('allowUserToManageServer')" />
                    <VCheckbox
                      v-model="model.IsDisabled"
                      :label="t('disableUser')" />
                    <VCheckbox
                      v-model="model.IsHidden"
                      :label="t('hideUserFromLogin')" />
                    <VCheckbox
                      v-model="model.EnableUserPreferenceAccess"
                      :label="t('allowUserPreferences')" />
                    <VCheckbox
                      v-model="model.EnableRemoteAccess"
                      :label="t('allowRemoteAccess')" />
                  </VCol>
                  <VCol
                    cols="12"
                    md="6">
                    <VTextField
                      v-model.number="model.RemoteClientBitrateLimit"
                      type="number"
                      min="0"
                      :label="t('remoteClientBitrateLimit')" />
                    <VTextField
                      v-model.number="model.MaxActiveSessions"
                      type="number"
                      min="0"
                      :label="t('maxActiveSessions')" />
                    <VTextField
                      v-model.number="model.SimultaneousStreamLimit"
                      type="number"
                      min="0"
                      :label="t('simultaneousStreamLimit')" />
                    <VTextField
                      v-model.number="model.LoginAttemptsBeforeLockout"
                      type="number"
                      min="-1"
                      :label="t('loginAttemptsBeforeLockout')" />
                    <VTextField
                      v-model.number="model.AutoRemoteQuality"
                      type="number"
                      min="0"
                      :label="t('autoRemoteQuality')" />
                  </VCol>
                </VRow>
                <VRow>
                  <VCol
                    cols="12"
                    md="6">
                    <div class="text-subtitle-1 text--secondary uno-font-medium">
                      {{ t('featureAccess') }}
                    </div>
                    <VCheckbox
                      v-model="model.EnableContentDeletion"
                      :label="t('allowContentDeletion')" />
                    <VCheckbox
                      v-model="model.EnableContentDownloading"
                      :label="t('allowContentDownloading')" />
                    <VCheckbox
                      v-model="model.EnableSubtitleDownloading"
                      :label="t('allowSubtitleDownloading')" />
                    <VCheckbox
                      v-model="model.EnableLiveTvAccess"
                      :label="t('allowLiveTvAccess')" />
                    <VCheckbox
                      v-model="model.EnableLiveTvManagement"
                      :label="t('allowLiveTvManagement')" />
                    <VCheckbox
                      v-model="model.EnableCollectionManagement"
                      :label="t('allowCollectionManagement')" />
                    <VCheckbox
                      v-model="model.EnableSubtitleManagement"
                      :label="t('allowSubtitleManagement')" />
                    <VCheckbox
                      v-model="model.EnableLyricManagement"
                      :label="t('allowLyricManagement')" />
                  </VCol>
                  <VCol
                    cols="12"
                    md="6">
                    <div class="text-subtitle-1 text--secondary uno-font-medium">
                      {{ t('playback') }}
                    </div>
                    <VCheckbox
                      v-model="model.EnableMediaPlayback"
                      :label="t('allowMediaPlayback')" />
                    <VCheckbox
                      v-model="model.EnableAudioPlaybackTranscoding"
                      :label="t('allowAudioPlaybackTranscoding')" />
                    <VCheckbox
                      v-model="model.EnableVideoPlaybackTranscoding"
                      :label="t('allowVideoPlaybackTranscoding')" />
                    <VCheckbox
                      v-model="model.EnablePlaybackRemuxing"
                      :label="t('allowPlaybackRemuxing')" />
                    <VCheckbox
                      v-model="model.EnableMediaConversion"
                      :label="t('allowMediaConversion')" />
                  </VCol>
                </VRow>
                <VRow>
                  <VCol
                    cols="12"
                    md="6">
                    <div class="text-subtitle-1 text--secondary uno-font-medium">
                      {{ t('remoteControl') }}
                    </div>
                    <VCheckbox
                      v-model="model.EnableRemoteControlOfOtherUsers"
                      :label="t('allowRemoteControlOthers')" />
                    <VCheckbox
                      v-model="model.EnableSharedDeviceControl"
                      :label="t('allowRemoteSharedDevices')" />
                  </VCol>
                  <VCol
                    cols="12"
                    md="6">
                    <div class="text-subtitle-1 text--secondary uno-font-medium">
                      {{ t('sync') }}
                    </div>
                    <VCheckbox
                      v-model="model.EnableSync"
                      :label="t('allowSyncContent')" />
                    <VCheckbox
                      v-model="model.EnableSyncTranscoding"
                      :label="t('allowSyncTranscoding')" />
                    <VCheckbox
                      v-model="model.EnablePublicSharing"
                      :label="t('allowPublicSharing')" />
                    <VCheckbox
                      v-model="model.AllowSharingPersonalItems"
                      :label="t('allowSharingPersonalItems')" />
                    <VCheckbox
                      v-model="model.AllowCameraUpload"
                      :label="t('allowCameraUpload')" />
                    <VCombobox
                      v-model="model.RestrictedFeatures"
                      :label="t('restrictedFeatures')"
                      multiple
                      chips
                      clearable />
                  </VCol>
                </VRow>
                <VRow>
                  <VCol>
                    <VBtn
                      :loading="loading"
                      color="error"
                      variant="elevated"
                      @click="deleteUser">
                      {{ t('deleteUser') }}
                    </VBtn>
                  </VCol>
                  <VCol>
                    <VBtn
                      :loading="loading"
                      color="primary"
                      variant="elevated"
                      class="uno-float-right"
                      @click="saveProfile">
                      {{ t('save') }}
                    </VBtn>
                  </VCol>
                </VRow>
              </VContainer>
            </VForm>
          </VWindowItem>
          <VWindowItem
            :key="2"
            :value="2">
            <VForm>
              <VContainer>
                <VRow>
                  <VCol>
                    <VCheckbox
                      v-model="model.CanAccessAllLibraries"
                      :label="t('allLibraries')" />
                  </VCol>
                </VRow>
                <div v-if="!model.CanAccessAllLibraries && libraries">
                  <VRow>
                    <div
                      class="text-subtitle-1 text--secondary uno-font-medium uno-capitalize">
                      {{ $t('libraries') }}
                    </div>
                  </VRow>
                  <VRow
                    v-for="library of libraries.Items"
                    :key="library.Id">
                    <VCol>
                      <VCheckbox
                        v-model="model.Folders"
                        :value="library.Id"
                        :label="library.Name!" />
                    </VCol>
                  </VRow>
                </div>
                <div v-if="model.EnableContentDeletion && libraries?.Items?.length">
                  <VRow>
                    <VCol>
                      <div class="text-subtitle-1 text--secondary uno-font-medium uno-capitalize">
                        {{ t('contentDeletionLibraries') }}
                      </div>
                    </VCol>
                  </VRow>
                  <VRow
                    v-for="library of libraries.Items"
                    :key="`delete-${library.Id}`">
                    <VCol>
                      <VCheckbox
                        v-model="model.ContentDeletionFolders"
                        :value="library.Id"
                        :label="library.Name!" />
                    </VCol>
                  </VRow>
                </div>
                <VRow>
                  <VCol
                    cols="12"
                    md="6">
                    <VCheckbox
                      v-model="model.CanAccessAllChannels"
                      :label="t('allChannels')" />
                  </VCol>
                  <VCol
                    cols="12"
                    md="6">
                    <VCheckbox
                      v-model="model.CanAccessAllDevices"
                      :label="t('allDevices')" />
                  </VCol>
                </VRow>
                <div v-if="!model.CanAccessAllChannels && channels.length">
                  <VRow>
                    <VCol>
                      <div class="text-subtitle-1 text--secondary uno-font-medium uno-capitalize">
                        {{ t('channels') }}
                      </div>
                    </VCol>
                  </VRow>
                  <VRow
                    v-for="channel of channels"
                    :key="channel.Id">
                    <VCol>
                      <VCheckbox
                        v-model="model.Channels"
                        :value="channel.Id"
                        :label="channel.Name || channel.Id" />
                    </VCol>
                  </VRow>
                </div>
                <VRow v-if="!model.CanAccessAllDevices">
                  <VCol>
                    <div class="text-subtitle-1 text--secondary uno-font-medium uno-capitalize">
                      {{ t('devices') }}
                    </div>
                    <VRow
                      v-for="device of devices"
                      :key="device.Id">
                      <VCol>
                        <VCheckbox
                          v-model="model.Devices"
                          :value="device.Id"
                          :label="formatDeviceName(device)" />
                      </VCol>
                    </VRow>
                    <VCombobox
                      v-if="!devices.length"
                      v-model="model.Devices"
                      :label="t('enabledDevices')"
                      multiple
                      chips
                      clearable />
                  </VCol>
                </VRow>
                <VRow>
                  <VCol>
                    <VBtn
                      :loading="loading"
                      color="primary"
                      variant="elevated"
                      class="uno-float-right"
                      @click="saveAccess">
                      {{ t('save') }}
                    </VBtn>
                  </VCol>
                </VRow>
              </VContainer>
            </VForm>
          </VWindowItem>
          <VWindowItem
            :key="3"
            :value="3">
            <VForm>
              <VContainer>
                <VRow>
                  <VCol>
                    <VSelect
                      v-model="model.maxParentalRating"
                      :label="t('maxAllowedRating')"
                      :items="parentalCategories"
                      item-title="label"
                      item-value="id"
                      hide-details
                      clearable />
                    <div
                      class="text-subtitle-1 text-warning uno-font-medium">
                      {{ $t('maxAllowedRatingSubtitle') }}
                    </div>
                  </VCol>
                </VRow>
                <VRow>
                  <VCol>
                    <div
                      class="text-subtitle-1 text--secondary uno-font-medium uno-capitalize">
                      {{ $t('blockUnratedItems') }}
                    </div>
                  </VCol>
                </VRow>
                <VContainer>
                  <VRow
                    v-for="cat of blockingCategories"
                    :key="cat.value"
                    dense>
                    <VCol>
                      <VCheckbox
                        v-model="model.BlockUnratedItems"
                        :label="cat.label"
                        :value="cat.value"
                        density="compact" />
                    </VCol>
                  </VRow>
                </VContainer>
                <VContainer>
                  <VRow>
                    <VCol>
                      <VCombobox
                        v-model="model.AllowedTags"
                        :label="t('allowedTags')"
                        multiple
                        chips
                        clearable />
                    </VCol>
                  </VRow>
                  <VRow>
                    <VCol>
                      <div
                        class="text-title uno-font-medium uno-capitalize">
                        {{ t('blockTags') }}
                      </div>
                    </VCol>
                    <VCol>
                      <VBtn
                        color="secondary"
                        variant="elevated"
                        @click="addTagDialogOpen = true">
                        {{ t('addBlockedTag') }}
                      </VBtn>
                    </VCol>
                  </VRow>
                  <VRow
                    v-for="blockedTag of model.BlockedTags"
                    :key="blockedTag">
                    <VCol>
                      <div
                        class="text-subtitle-1 uno-font-medium uno-capitalize">
                        {{ blockedTag }}
                      </div>
                    </VCol>
                    <VCol>
                      <VBtn
                        :disabled="loading"
                        color="error"
                        @click="() => model.BlockedTags = model.BlockedTags.filter(tag => tag !== blockedTag)">
                        {{ t('unblockTag') }}
                      </VBtn>
                    </VCol>
                  </VRow>
                </VContainer>
                <VRow>
                  <VCol>
                    <div class="uno-flex uno-items-center uno-justify-between uno-gap-3">
                      <div class="text-subtitle-1 text--secondary uno-font-medium">
                        {{ t('accessSchedules') }}
                      </div>
                      <VBtn
                        color="secondary"
                        variant="elevated"
                        @click="openScheduleDialog()">
                        {{ t('add') }}
                      </VBtn>
                    </div>
                    <VList
                      v-if="model.AccessSchedules.length"
                      class="uno-mt-2"
                      density="comfortable">
                      <VListItem
                        v-for="(schedule, index) of model.AccessSchedules"
                        :key="`${schedule.DayOfWeek}-${schedule.StartHour}-${schedule.EndHour}-${index}`"
                        :title="formatSchedule(schedule)">
                        <template #append>
                          <VBtn
                            variant="text"
                            @click="openScheduleDialog(index)">
                            {{ t('edit') }}
                          </VBtn>
                          <VBtn
                            color="error"
                            variant="text"
                            @click="removeSchedule(index)">
                            {{ t('delete') }}
                          </VBtn>
                        </template>
                      </VListItem>
                    </VList>
                    <div
                      v-else
                      class="uno-mt-2 text-medium-emphasis">
                      {{ t('noAccessSchedules') }}
                    </div>
                  </VCol>
                </VRow>
                <VRow>
                  <VCol>
                    <VBtn
                      :loading="loading"
                      color="primary"
                      variant="elevated"
                      class="uno-float-right"
                      @click="saveParentalControl">
                      {{ t('save') }}
                    </VBtn>
                  </VCol>
                </VRow>
              </VContainer>
            </VForm>
          </VWindowItem>
          <VWindowItem
            :key="4"
            :value="4">
            <VForm>
              <VContainer>
                <VRow>
                  <VCol>
                    <VTextField
                      v-if="user.HasPassword"
                      v-model="model.CurrentPassword"
                      :disabled="loading"
                      :label="t('currentPassword')"
                      hide-details />
                  </VCol>
                </VRow>
                <VRow>
                  <VCol>
                    <VTextField
                      v-model="model.Password"
                      :disabled="loading"
                      :label="t('newPassword')"
                      hide-details />
                  </VCol>
                </VRow>
                <VRow>
                  <VCol>
                    <VTextField
                      v-model="model.ConfirmPassword"
                      :disabled="loading"
                      :label="t('confirmPassword')"
                      hide-details />
                  </VCol>
                </VRow>
                <VRow>
                  <VCol>
                    <VBtn
                      v-if="user.HasPassword"
                      :disabled="loading"
                      :loading="loading"
                      variant="elevated"
                      color="error"
                      @click="resetPassword">
                      {{ t('resetPassword') }}
                    </VBtn>
                  </VCol>
                  <VCol>
                    <VBtn
                      :disabled="loading"
                      variant="elevated"
                      color="primary"
                      class="uno-float-right"
                      @click="submitPassword">
                      {{ t('save') }}
                    </VBtn>
                  </VCol>
                </VRow>
              </VContainer>
            </VForm>
          </VWindowItem>
        </VWindow>
      </VCard>
      <VDialog
        v-model="addTagDialogOpen"
        width="auto">
        <VCol class="add-key-dialog uno-p-0">
          <VCard>
            <VCardTitle>{{ t('addBlockedTag') }}</VCardTitle>
            <VCardActions>
              <VForm
                class="add-key-form"
                @submit.prevent="model.BlockedTags.push(newTagValue); addTagDialogOpen = false; newTagValue = '';">
                <VTextField
                  v-model="newTagValue"
                  variant="outlined"
                  :label="t('tagName')" />
                <VBtn
                  color="primary"
                  :loading="loading"
                  :disabled="newTagValue === ''"
                  @click="model.BlockedTags.push(newTagValue); addTagDialogOpen = false; newTagValue = '';">
                  {{ $t('confirm') }}
                </VBtn>
                <VBtn @click="() => {addTagDialogOpen = false}">
                  {{ $t('cancel') }}
                </VBtn>
              </VForm>
            </VCardActions>
          </VCard>
        </VCol>
      </VDialog>
      <VDialog
        v-model="scheduleDialogOpen"
        max-width="520">
        <VCard>
          <VCardTitle>{{ t('accessSchedule') }}</VCardTitle>
          <VCardText>
            <VSelect
              v-model="scheduleForm.DayOfWeek"
              :label="t('accessDay')"
              :items="scheduleDays"
              item-title="title"
              item-value="value" />
            <VSelect
              v-model="scheduleForm.StartHour"
              :label="t('accessStart')"
              :items="scheduleHours"
              item-title="title"
              item-value="value" />
            <VSelect
              v-model="scheduleForm.EndHour"
              :label="t('accessEnd')"
              :items="scheduleHours"
              item-title="title"
              item-value="value" />
          </VCardText>
          <VCardActions>
            <VSpacer />
            <VBtn
              variant="text"
              @click="scheduleDialogOpen = false">
              {{ t('cancel') }}
            </VBtn>
            <VBtn
              color="primary"
              variant="elevated"
              @click="saveScheduleForm">
              {{ t('save') }}
            </VBtn>
          </VCardActions>
        </VCard>
      </VDialog>
    </template>
  </SettingsPage>
</template>

<route lang="yaml">
meta:
  admin: true
</route>

<script setup lang="ts">
import type {
  BaseItemDtoQueryResult,
  DevicesDeviceInfo,
  UnratedItem,
  UserDto,
  UserPolicy
} from '@jellyfin/sdk/lib/generated-client';
import { getLibraryApi } from '@jellyfin/sdk/lib/utils/api/library-api';
import { getLocalizationApi } from '@jellyfin/sdk/lib/utils/api/localization-api';
import { getUserApi } from '@jellyfin/sdk/lib/utils/api/user-api';
import { computed, ref } from 'vue';
import { useTranslation } from 'i18next-vue';
import { useRoute, useRouter } from 'vue-router';
import { remote } from '#/plugins/remote/index.ts';
import { useSnackbar } from '#/composables/use-snackbar.ts';
import { useConfirmDialog } from '#/composables/use-confirm-dialog.ts';
import { useSettingsSdk, type SettingsChannelInfo } from '#/composables/use-settings-sdk.ts';

type AccessSchedule = {
  DayOfWeek: string;
  StartHour: number;
  EndHour: number;
};

interface CurrentUser {
  Name: string;
  CurrentPassword: string;
  Password: string;
  ConfirmPassword: string;
  IsAdministrator: boolean;
  IsDisabled: boolean;
  IsHidden: boolean;
  EnableRemoteAccess: boolean;
  EnableUserPreferenceAccess: boolean;
  EnableContentDeletion: boolean;
  EnableContentDownloading: boolean;
  EnableSubtitleDownloading: boolean;
  EnableLiveTvAccess: boolean;
  EnableLiveTvManagement: boolean;
  EnableCollectionManagement: boolean;
  EnableSubtitleManagement: boolean;
  EnableLyricManagement: boolean;
  EnableMediaPlayback: boolean;
  EnableAudioPlaybackTranscoding: boolean;
  EnableVideoPlaybackTranscoding: boolean;
  EnablePlaybackRemuxing: boolean;
  EnableMediaConversion: boolean;
  EnableRemoteControlOfOtherUsers: boolean;
  EnableSharedDeviceControl: boolean;
  EnableSync: boolean;
  EnableSyncTranscoding: boolean;
  EnablePublicSharing: boolean;
  AllowSharingPersonalItems: boolean;
  AllowCameraUpload: boolean;
  RestrictedFeatures: string[];
  AutoRemoteQuality: number;
  RemoteClientBitrateLimit: number;
  MaxActiveSessions: number;
  SimultaneousStreamLimit: number;
  LoginAttemptsBeforeLockout: number;
  CanAccessAllLibraries: boolean;
  CanAccessAllChannels: boolean;
  CanAccessAllDevices: boolean;
  Folders: string[];
  ContentDeletionFolders: string[];
  Channels: string[];
  Devices: string[];
  maxParentalRating?: number;
  BlockUnratedItems: UnratedItem[];
  BlockedTags: string[];
  AllowedTags: string[];
  AccessSchedules: AccessSchedule[];
}

const { t } = useTranslation();
const route = useRoute('/settings/users/[id]');
const router = useRouter();
const { channelsApi, devicesApi } = useSettingsSdk();

const loading = ref<boolean>(false);
const addTagDialogOpen = ref<boolean>(false);
const scheduleDialogOpen = ref<boolean>(false);
const scheduleEditIndex = ref<number | null>(null);
const newTagValue = ref<string>('');
const user = ref<UserDto>({});
const libraries = ref<BaseItemDtoQueryResult>();
const channels = ref<SettingsChannelInfo[]>([]);
const devices = ref<DevicesDeviceInfo[]>([]);
const parentalCategories = ref<{ label: string; id: number | undefined }[]>([]);
const scheduleForm = ref<AccessSchedule>({
  DayOfWeek: 'Sunday',
  StartHour: 0,
  EndHour: 24
});
const model = ref<CurrentUser>({
  Name: '',
  CurrentPassword: '',
  Password: '',
  ConfirmPassword: '',
  IsAdministrator: false,
  IsDisabled: false,
  IsHidden: false,
  EnableRemoteAccess: true,
  EnableUserPreferenceAccess: true,
  EnableContentDeletion: false,
  EnableContentDownloading: true,
  EnableSubtitleDownloading: true,
  EnableLiveTvAccess: false,
  EnableLiveTvManagement: false,
  EnableCollectionManagement: false,
  EnableSubtitleManagement: false,
  EnableLyricManagement: false,
  EnableMediaPlayback: true,
  EnableAudioPlaybackTranscoding: true,
  EnableVideoPlaybackTranscoding: true,
  EnablePlaybackRemuxing: true,
  EnableMediaConversion: false,
  EnableRemoteControlOfOtherUsers: false,
  EnableSharedDeviceControl: false,
  EnableSync: false,
  EnableSyncTranscoding: false,
  EnablePublicSharing: true,
  AllowSharingPersonalItems: false,
  AllowCameraUpload: false,
  RestrictedFeatures: [],
  AutoRemoteQuality: 0,
  RemoteClientBitrateLimit: 0,
  MaxActiveSessions: 0,
  SimultaneousStreamLimit: 0,
  LoginAttemptsBeforeLockout: -1,
  CanAccessAllLibraries: false,
  CanAccessAllChannels: true,
  CanAccessAllDevices: true,
  Folders: [],
  ContentDeletionFolders: [],
  Channels: [],
  Devices: [],
  maxParentalRating: undefined,
  BlockUnratedItems: [],
  BlockedTags: [],
  AllowedTags: [],
  AccessSchedules: []
});
const tab = ref<number>(1);
const blockingCategories = computed(() =>
  [
    {
      label: t('books'),
      value: 'Book'
    },
    {
      label: t('games'),
      value: 'Game'
    },
    {
      label: t('channels'),
      value: 'ChannelContent'
    },
    {
      label: t('liveTv'),
      value: 'LiveTvChannel'
    },
    {
      label: t('movies'),
      value: 'Movie'
    },
    {
      label: t('music'),
      value: 'Music'
    },
    {
      label: t('trailer'),
      value: 'Trailer'
    },
    {
      label: t('shows'),
      value: 'Series'
    },
    {
      label: t('other'),
      value: 'Other'
    }]
);
const scheduleDays = computed(() => [
  { title: t('sunday'), value: 'Sunday' },
  { title: t('monday'), value: 'Monday' },
  { title: t('tuesday'), value: 'Tuesday' },
  { title: t('wednesday'), value: 'Wednesday' },
  { title: t('thursday'), value: 'Thursday' },
  { title: t('friday'), value: 'Friday' },
  { title: t('saturday'), value: 'Saturday' },
  { title: t('everyday'), value: 'Everyday' },
  { title: t('weekdays'), value: 'Weekday' },
  { title: t('weekends'), value: 'Weekend' }
]);
const scheduleHours = computed(() => Array.from({ length: 25 }, (_, hour) => ({
  title: hour === 24 ? '24:00' : `${hour.toString().padStart(2, '0')}:00`,
  value: hour
})));

/**
 * Loads all data required for this page
 */
async function load(): Promise<void> {
  const { id } = route.params;

  user.value = (await remote.sdk.newUserApi(getUserApi).getUserById({
    userId: id
  })).data;
  initializeUser();
  libraries.value = (await remote.sdk.newUserApi(getLibraryApi).getMediaFolders({ isHidden: false })).data;
  const [channelItems, deviceResult] = await Promise.all([
    channelsApi.getChannels(),
    devicesApi.getDevices()
  ]);

  channels.value = channelItems;
  devices.value = deviceResult.data.Items ?? [];

  const cats = (await remote.sdk.newUserApi(getLocalizationApi).getParentalRatings()).data;

  for (const cat of cats) {
    if (parentalCategories.value.some(c => c.id === cat.Value!)) {
      parentalCategories.value = parentalCategories.value.map((c) => {
        if (c.id === cat.Value!) {
          return { label: `${c.label}/${cat.Name!}`, id: cat.Value };
        }

        return c;
      });
    } else {
      parentalCategories.value.push({ label: cat.Name!, id: cat.Value! });
    }
  }
}

await load();

/**
 * Saves the changed user access
 */
async function saveAccess(): Promise<void> {
  if (!user.value.Id) {
    return;
  }

  loading.value = true;
  await remote.sdk.newUserApi(getUserApi).updateUserPolicy({
    userId: user.value.Id,
    userPolicy: {
      ...user.value.Policy as UserPolicy,
      EnableAllFolders: model.value.CanAccessAllLibraries,
      EnabledFolders: model.value.CanAccessAllLibraries ? [] : model.value.Folders,
      EnableAllChannels: model.value.CanAccessAllChannels,
      EnabledChannels: model.value.CanAccessAllChannels ? [] : model.value.Channels,
      EnableAllDevices: model.value.CanAccessAllDevices,
      EnabledDevices: model.value.CanAccessAllDevices ? [] : model.value.Devices,
      EnableContentDeletionFromFolders: model.value.EnableContentDeletion ? model.value.ContentDeletionFolders : [],
      BlockedChannels: null,
      BlockedMediaFolders: null
    } as UserPolicy
  });
  await refreshData();
  loading.value = false;
}

/**
 * Saves the changed profile
 */
async function saveProfile(): Promise<void> {
  if (!user.value.Id) {
    return;
  }

  loading.value = true;
  await remote.sdk.newUserApi(getUserApi).updateUser({
    userId: user.value.Id,
    userDto: { ...user.value, Name: model.value.Name }
  });
  await remote.sdk.newUserApi(getUserApi).updateUserPolicy({
    userId: user.value.Id,
    userPolicy: {
      ...user.value.Policy as UserPolicy,
      IsAdministrator: model.value.IsAdministrator,
      IsDisabled: model.value.IsDisabled,
      IsHidden: model.value.IsHidden,
      EnableRemoteAccess: model.value.EnableRemoteAccess,
      EnableUserPreferenceAccess: model.value.EnableUserPreferenceAccess,
      EnableContentDeletion: model.value.EnableContentDeletion,
      EnableContentDownloading: model.value.EnableContentDownloading,
      EnableSubtitleDownloading: model.value.EnableSubtitleDownloading,
      EnableLiveTvAccess: model.value.EnableLiveTvAccess,
      EnableLiveTvManagement: model.value.EnableLiveTvManagement,
      EnableCollectionManagement: model.value.EnableCollectionManagement,
      EnableSubtitleManagement: model.value.EnableSubtitleManagement,
      EnableLyricManagement: model.value.EnableLyricManagement,
      EnableMediaPlayback: model.value.EnableMediaPlayback,
      EnableAudioPlaybackTranscoding: model.value.EnableAudioPlaybackTranscoding,
      EnableVideoPlaybackTranscoding: model.value.EnableVideoPlaybackTranscoding,
      EnablePlaybackRemuxing: model.value.EnablePlaybackRemuxing,
      EnableMediaConversion: model.value.EnableMediaConversion,
      EnableRemoteControlOfOtherUsers: model.value.EnableRemoteControlOfOtherUsers,
      EnableSharedDeviceControl: model.value.EnableSharedDeviceControl,
      EnableSync: model.value.EnableSync,
      EnableSyncTranscoding: model.value.EnableSyncTranscoding,
      EnablePublicSharing: model.value.EnablePublicSharing,
      AllowSharingPersonalItems: model.value.AllowSharingPersonalItems,
      AllowCameraUpload: model.value.AllowCameraUpload,
      RestrictedFeatures: model.value.RestrictedFeatures,
      AutoRemoteQuality: model.value.AutoRemoteQuality || 0,
      RemoteClientBitrateLimit: model.value.RemoteClientBitrateLimit || 0,
      MaxActiveSessions: model.value.MaxActiveSessions || 0,
      SimultaneousStreamLimit: model.value.SimultaneousStreamLimit || 0,
      LoginAttemptsBeforeLockout: model.value.LoginAttemptsBeforeLockout
    } as UserPolicy
  });
  await refreshData();
  loading.value = false;
}

/**
 * Saves the changed parental control
 */
async function saveParentalControl(): Promise<void> {
  if (!user.value.Id) {
    return;
  }

  loading.value = true;
  await remote.sdk.newUserApi(getUserApi).updateUserPolicy({
    userId: user.value.Id,
    userPolicy: {
      ...user.value.Policy as UserPolicy,
      MaxParentalRating: model.value.maxParentalRating,
      BlockUnratedItems: model.value.BlockUnratedItems,
      BlockedTags: model.value.BlockedTags,
      AllowedTags: model.value.AllowedTags,
      AccessSchedules: model.value.AccessSchedules
    } as UserPolicy
  });
  loading.value = false;
}

function openScheduleDialog(index?: number): void {
  scheduleEditIndex.value = index ?? null;
  const source = index === undefined ? undefined : model.value.AccessSchedules[index];
  scheduleForm.value = {
    DayOfWeek: source?.DayOfWeek ?? 'Sunday',
    StartHour: Number(source?.StartHour ?? 0),
    EndHour: Number(source?.EndHour ?? 24)
  };
  scheduleDialogOpen.value = true;
}

function saveScheduleForm(): void {
  if (Number(scheduleForm.value.StartHour) >= Number(scheduleForm.value.EndHour)) {
    useSnackbar(t('startHourMustBeBeforeEnd'), 'error');

    return;
  }

  const next = [...model.value.AccessSchedules];
  const value = {
    DayOfWeek: scheduleForm.value.DayOfWeek,
    StartHour: Number(scheduleForm.value.StartHour),
    EndHour: Number(scheduleForm.value.EndHour)
  };

  if (scheduleEditIndex.value === null) {
    next.push(value);
  } else {
    next[scheduleEditIndex.value] = value;
  }

  model.value.AccessSchedules = next;
  scheduleDialogOpen.value = false;
  scheduleEditIndex.value = null;
}

function removeSchedule(index: number): void {
  model.value.AccessSchedules = model.value.AccessSchedules.filter((_, itemIndex) => itemIndex !== index);
}

function formatSchedule(schedule: AccessSchedule): string {
  const day = scheduleDays.value.find(item => item.value === schedule.DayOfWeek)?.title ?? schedule.DayOfWeek;

  return `${day} ${formatHour(schedule.StartHour)} - ${formatHour(schedule.EndHour)}`;
}

function formatHour(hour: number): string {
  return hour === 24 ? '24:00' : `${Number(hour).toString().padStart(2, '0')}:00`;
}

function formatDeviceName(device: DevicesDeviceInfo): string {
  const app = [device.AppName, device.AppVersion].filter(Boolean).join(' ');

  return [device.Name || device.Id, app].filter(Boolean).join(' - ');
}

/**
 * Saves the changed password
 */
async function submitPassword(): Promise<void> {
  if (!user.value.Id) {
    return;
  }

  if (!model.value.Password || model.value.Password !== model.value.ConfirmPassword) {
    useSnackbar(t('bothPasswordsSame'), 'error');

    return;
  }

  loading.value = true;
  await remote.sdk.newUserApi(getUserApi).updateUserPassword({ userId: user.value.Id, updateUserPassword: { NewPw: model.value.Password, ...(user.value.HasPassword && { CurrentPw: model.value.CurrentPassword }) } });
  model.value = { ...model.value, CurrentPassword: '', Password: '', ConfirmPassword: '' };
  await refreshData();
  loading.value = false;
}

/**
 * Refreshes the user data
 */
async function refreshData(): Promise<void> {
  if (!user.value.Id) {
    return;
  }

  user.value = (await remote.sdk.newUserApi(getUserApi).getUserById({
    userId: user.value.Id
  })).data;
  initializeUser();
}

/**
 * Deletes the user
 */
async function deleteUser(): Promise<void> {
  await useConfirmDialog(async () => {
    await remote.sdk.newUserApi(getUserApi).deleteUser({ userId: user.value.Id! });
    await router.push('/settings/users');
  }, {
    title: t('deleteUser'),
    text: t('deleteUserConfirm'),
    confirmText: t('delete')
  });
}

/**
 * Resets the password
 */
async function resetPassword(): Promise<void> {
  if (!user.value.Id) {
    return;
  }

  loading.value = true;
  await remote.sdk.newUserApi(getUserApi).updateUserPassword({ userId: user.value.Id, updateUserPassword: {
    ResetPassword: true
  } });
  await refreshData();
  loading.value = false;
}

/**
 * This function makes sure that all properties are defined
 */
function initializeUser(): void {
  const policy = user.value.Policy as UserPolicy & Record<string, unknown> | undefined;
  const accessSchedules = Array.isArray(policy?.AccessSchedules)
    ? policy.AccessSchedules
        .map(schedule => normalizeAccessSchedule(schedule))
        .filter((schedule): schedule is AccessSchedule => Boolean(schedule))
    : [];

  model.value = {
    ...model.value,
    IsAdministrator: Boolean(policy?.IsAdministrator),
    IsDisabled: Boolean(policy?.IsDisabled),
    IsHidden: Boolean(policy?.IsHidden),
    EnableRemoteAccess: policy?.EnableRemoteAccess as boolean ?? true,
    EnableUserPreferenceAccess: policy?.EnableUserPreferenceAccess as boolean ?? true,
    EnableContentDeletion: Boolean(policy?.EnableContentDeletion),
    EnableContentDownloading: policy?.EnableContentDownloading as boolean ?? true,
    EnableSubtitleDownloading: policy?.EnableSubtitleDownloading as boolean ?? true,
    EnableLiveTvAccess: Boolean(policy?.EnableLiveTvAccess),
    EnableLiveTvManagement: Boolean(policy?.EnableLiveTvManagement),
    EnableCollectionManagement: Boolean(policy?.EnableCollectionManagement),
    EnableSubtitleManagement: Boolean(policy?.EnableSubtitleManagement),
    EnableLyricManagement: Boolean(policy?.EnableLyricManagement),
    EnableMediaPlayback: policy?.EnableMediaPlayback as boolean ?? true,
    EnableAudioPlaybackTranscoding: policy?.EnableAudioPlaybackTranscoding as boolean ?? true,
    EnableVideoPlaybackTranscoding: policy?.EnableVideoPlaybackTranscoding as boolean ?? true,
    EnablePlaybackRemuxing: policy?.EnablePlaybackRemuxing as boolean ?? true,
    EnableMediaConversion: Boolean(policy?.EnableMediaConversion),
    EnableRemoteControlOfOtherUsers: Boolean(policy?.EnableRemoteControlOfOtherUsers),
    EnableSharedDeviceControl: Boolean(policy?.EnableSharedDeviceControl),
    EnableSync: Boolean(policy?.EnableSync),
    EnableSyncTranscoding: Boolean(policy?.EnableSyncTranscoding),
    EnablePublicSharing: policy?.EnablePublicSharing as boolean ?? true,
    AllowSharingPersonalItems: Boolean(policy?.AllowSharingPersonalItems),
    AllowCameraUpload: Boolean(policy?.AllowCameraUpload),
    RestrictedFeatures: policy?.RestrictedFeatures as string[] ?? [],
    AutoRemoteQuality: Number(policy?.AutoRemoteQuality ?? 0),
    RemoteClientBitrateLimit: Number(policy?.RemoteClientBitrateLimit ?? 0),
    MaxActiveSessions: Number(policy?.MaxActiveSessions ?? 0),
    SimultaneousStreamLimit: Number(policy?.SimultaneousStreamLimit ?? 0),
    LoginAttemptsBeforeLockout: Number(policy?.LoginAttemptsBeforeLockout ?? -1),
    CanAccessAllLibraries: policy?.EnableAllFolders as boolean ?? false,
    CanAccessAllChannels: policy?.EnableAllChannels as boolean ?? true,
    CanAccessAllDevices: policy?.EnableAllDevices as boolean ?? true,
    Name: user.value.Name ?? '',
    Folders: policy?.EnabledFolders as string[] ?? [],
    ContentDeletionFolders: policy?.EnableContentDeletionFromFolders as string[] ?? [],
    Channels: policy?.EnabledChannels as string[] ?? [],
    Devices: policy?.EnabledDevices as string[] ?? [],
    maxParentalRating: policy?.MaxParentalRating as number | undefined ?? undefined,
    BlockUnratedItems: policy?.BlockUnratedItems as UnratedItem[] ?? [],
    BlockedTags: policy?.BlockedTags as string[] ?? [],
    AllowedTags: policy?.AllowedTags as string[] ?? [],
    AccessSchedules: accessSchedules
  };
}

function normalizeAccessSchedule(value: unknown): AccessSchedule | null {
  if (!value || typeof value !== 'object') {
    return null;
  }

  const schedule = value as Record<string, unknown>;
  const day = typeof schedule.DayOfWeek === 'string' ? schedule.DayOfWeek : 'Sunday';
  const start = Number(schedule.StartHour ?? 0);
  const end = Number(schedule.EndHour ?? 24);

  if (Number.isNaN(start) || Number.isNaN(end)) {
    return null;
  }

  return {
    DayOfWeek: day,
    StartHour: start,
    EndHour: end
  };
}
</script>
