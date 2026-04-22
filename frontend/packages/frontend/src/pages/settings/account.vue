<template>
  <SettingsPage>
    <template #title>
      {{ t('account') }}
    </template>

    <template #content>
      <VCol
        cols="12"
        md="10"
        lg="6">
        <div class="uno-mb-10 uno-flex uno-flex-col sm:uno-flex-row">
          <UserImage :size="190" />
          <div class="sm:uno-ml-10">
            <div class="uno-mb-7 uno-ml-2 uno-mt-6 uno-text-4xl sm:uno-mt-0">
              {{ remote.auth.currentUser.value?.Name }}
            </div>
            <div class="uno-flex uno-flex-col sm:uno-flex-row">
              <JFileUpload
                v-model="selectedUserPicture"
                :loading="isChangeImageLoading"
                type="button"
                block
                :button-text="t('changeImage')"
                accept="image/*" />
              <VBtn
                :loading="isDeleteImageLoading"
                variant="flat"
                size="large"
                class="uno-ml-0 uno-mt-6 sm:uno-ml-4 sm:uno-mt-0"
                color="error"
                @click="deleteUserImage">
                {{ t('deleteImage') }}
              </VBtn>
            </div>
          </div>
        </div>
        <div>
          <VTextField
            v-model="currentPassword"
            variant="outlined"
            class="uno-mb-2"
            :label="$t('currentPassword')"
            type="password" />
          <VTextField
            v-model="newPassword"
            variant="outlined"
            class="uno-mb-2"
            :label="$t('newPassword')"
            type="password" />
          <VTextField
            v-model="repeatNewPassword"
            variant="outlined"
            class="uno-mb-2"
            :label="$t('confirmPassword')"
            type="password" />
          <VBtn
            :loading="isChangePasswordLoading"
            :block="$vuetify.display.mobile"
            variant="flat"
            size="large"
            color="primary"
            @click="changePassword">
            {{ t('changePassword') }}
          </VBtn>
        </div>
      </VCol>

      <VCol
        cols="12"
        md="8"
        lg="4">
        <VTable>
          <tbody>
            <tr>
              <td>{{ t('userName') }}</td>
              <td>{{ remote.auth.currentUser.value?.Name }}</td>
            </tr>
            <tr>
              <td>{{ t('userId') }}</td>
              <td>{{ remote.auth.currentUser.value?.Id }}</td>
            </tr>
            <tr>
              <td>{{ t('administrator') }}</td>
              <td>{{ remote.auth.currentUser.value?.Policy?.IsAdministrator ? t('yes') : t('no') }}</td>
            </tr>
            <tr>
              <td>{{ t('disabled') }}</td>
              <td>{{ remote.auth.currentUser.value?.Policy?.IsDisabled ? t('yes') : t('no') }}</td>
            </tr>
            <tr>
              <td>{{ t('hidden') }}</td>
              <td>{{ remote.auth.currentUser.value?.Policy?.IsHidden ? t('yes') : t('no') }}</td>
            </tr>
            <tr>
              <td>{{ t('hasPassword') }}</td>
              <td>{{ remote.auth.currentUser.value?.HasPassword ? t('yes') : t('no') }}</td>
            </tr>
            <tr>
              <td>{{ t('autoPlayNextEpisode') }}</td>
              <td>{{ remote.auth.currentUser.value?.Configuration?.EnableNextEpisodeAutoPlay ? t('enabled') : t('disabled') }}</td>
            </tr>
            <tr>
              <td>{{ t('playDefaultAudioTrack') }}</td>
              <td>{{ remote.auth.currentUser.value?.Configuration?.PlayDefaultAudioTrack ? t('enabled') : t('disabled') }}</td>
            </tr>
            <tr>
              <td>{{ t('playDefaultSubtitleTrack') }}</td>
              <td>{{ remote.auth.currentUser.value?.Configuration?.PlayDefaultSubtitleTrack ? t('enabled') : t('disabled') }}</td>
            </tr>
          </tbody>
        </VTable>
      </VCol>
    </template>
  </SettingsPage>
</template>

<script setup lang="ts">
import { useTranslation } from 'i18next-vue';
import { nextTick, ref, shallowRef, watch } from 'vue';
import type { UserApiUpdateUserPasswordRequest } from '@jellyfin/sdk/lib/generated-client/api/user-api';
import type { ImageApiPostUserImageRequest } from '@jellyfin/sdk/lib/generated-client/api/image-api';
import type { AxiosRequestConfig } from 'axios';
import { useConfirmDialog } from '../../composables/use-confirm-dialog';
import { useSettingsSdk } from '#/composables/use-settings-sdk.ts';
import { remote } from '#/plugins/remote/index.ts';
import { useSnackbar } from '#/composables/use-snackbar.ts';

const { t } = useTranslation();
const { accountApi } = useSettingsSdk();

const currentPassword = shallowRef('');
const newPassword = shallowRef('');
const repeatNewPassword = shallowRef('');

const isChangePasswordLoading = shallowRef(false);
const isChangeImageLoading = shallowRef(false);
const isDeleteImageLoading = shallowRef(false);

const selectedUserPicture = ref<File | undefined>(undefined);

async function deleteUserImage() {
  if (!remote.auth.currentUserId.value) {
    return;
  }

  await useConfirmDialog(async () => {
    isDeleteImageLoading.value = true;

    try {
      await accountApi.deleteUserImage(remote.auth.currentUserId.value!);
    } catch {
      useSnackbar(t('failedToDeleteImage'), 'red');
    } finally {
      isDeleteImageLoading.value = false;
    }

    await remote.auth.refreshCurrentUserInfo();
  }, {
    title: t('deleteImage'),
    text: t('deleteImageConfirm'),
    confirmText: t('delete')
  });
}

async function changeUserImage() {
  if (!selectedUserPicture.value) {
    useSnackbar(t('failedToReadImage'), 'red');
    return;
  }

  const payload: ImageApiPostUserImageRequest = {
    userId: remote.auth.currentUserId.value,
    body: selectedUserPicture.value
  };

  const config: AxiosRequestConfig = {
    headers: {
      'Content-Type': selectedUserPicture.value.type
    }
  };

  isChangeImageLoading.value = true;

  try {
    await accountApi.postUserImage(payload, config);
  } catch {
    useSnackbar(t('failedToChangeImage'), 'red');
  } finally {
    isChangeImageLoading.value = false;
  }

  selectedUserPicture.value = undefined;
  await remote.auth.refreshCurrentUserInfo();
}

async function changePassword() {
  if (newPassword.value !== repeatNewPassword.value) {
    useSnackbar(t('newPasswordAndConfirmNewPasswordMustBeTheSame'), 'red');
    return;
  }

  if (!remote.auth.currentUserId.value) {
    return;
  }

  const payload: UserApiUpdateUserPasswordRequest & { userId: string } = {
    userId: remote.auth.currentUserId.value,
    updateUserPassword: {
      CurrentPw: currentPassword.value,
      NewPw: newPassword.value
    }
  };

  try {
    isChangePasswordLoading.value = true;
    await accountApi.updateUserPassword(payload);

    newPassword.value = '';
    currentPassword.value = '';
    repeatNewPassword.value = '';

    useSnackbar(t('passwordChangedSuccessfully'), 'green');
    await remote.auth.refreshCurrentUserInfo();
  } catch {
    useSnackbar(t('passwordChangeFailed'), 'red');
  } finally {
    isChangePasswordLoading.value = false;
  }
}

watch(selectedUserPicture, async (newVal) => {
  if (newVal) {
    await nextTick();
    await changeUserImage();
  }
});
</script>
