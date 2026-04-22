import type { AxiosRequestConfig } from 'axios';
import type {
  ActivityLogEntry,
  AuthenticationInfo,
  BrandingBrandingOptions,
  DevicesDeviceInfo,
  DevicesDeviceOptions,
  GlobalizationLocalizatonOption,
  LogFile,
  QueryResultActivityLogEntry,
  QueryResultDevicesDeviceInfo,
  QueryResultString,
  ServerConfiguration,
  SystemInfo
} from '@jellyfin/sdk/lib/generated-client';
import type { ImageApiPostUserImageRequest } from '@jellyfin/sdk/lib/generated-client/api/image-api';
import type { UserApiUpdateUserPasswordRequest } from '@jellyfin/sdk/lib/generated-client/api/user-api';
import { getApiKeyApi } from '@jellyfin/sdk/lib/utils/api/api-key-api';
import { getBrandingApi } from '@jellyfin/sdk/lib/utils/api/branding-api';
import { getConfigurationApi } from '@jellyfin/sdk/lib/utils/api/configuration-api';
import { getDevicesApi } from '@jellyfin/sdk/lib/utils/api/devices-api';
import { getImageApi } from '@jellyfin/sdk/lib/utils/api/image-api';
import { getLocalizationApi } from '@jellyfin/sdk/lib/utils/api/localization-api';
import { getSystemApi } from '@jellyfin/sdk/lib/utils/api/system-api';
import { getUserApi } from '@jellyfin/sdk/lib/utils/api/user-api';
import RemotePluginAxiosInstance from '#/plugins/remote/axios.ts';
import { remote } from '#/plugins/remote/index.ts';

export type SettingsDeviceDetails = DevicesDeviceInfo & {
  ReportedDeviceId?: string;
};

export type SettingsSessionInfo = {
  Id: string;
  UserName?: string;
  Client?: string;
  DeviceId?: string;
  DeviceName?: string;
  ApplicationVersion?: string;
  RemoteEndPoint?: string;
  SupportsRemoteControl?: boolean;
  SupportedCommands?: string[];
  NowPlayingItem?: {
    Name?: string;
  };
  PlayState?: {
    IsPaused?: boolean;
  };
};

export function useSettingsSdk() {
  const accountApi = {
    deleteUserImage: (userId: string) =>
      (remote.sdk.newUserApi(getImageApi) as {
        deleteUserImage: (payload: { userId: string }) => Promise<unknown>;
      }).deleteUserImage({ userId }),
    postUserImage: (payload: ImageApiPostUserImageRequest, config?: AxiosRequestConfig) =>
      (remote.sdk.newUserApi(getImageApi) as {
        postUserImage: (request: ImageApiPostUserImageRequest, config?: AxiosRequestConfig) => Promise<unknown>;
      }).postUserImage(payload, config),
    updateUserPassword: (payload: UserApiUpdateUserPasswordRequest & { userId: string }) =>
      (remote.sdk.newUserApi(getUserApi) as {
        updateUserPassword: (request: UserApiUpdateUserPasswordRequest & { userId: string }) => Promise<unknown>;
      }).updateUserPassword(payload)
  };

  const logsApi = {
    getLogs: async (): Promise<LogFile[]> =>
      ((await (remote.sdk.newUserApi(getSystemApi) as {
        getSystemLogs: () => Promise<{ data: LogFile[] }>;
      }).getSystemLogs()).data ?? []),
    getLogLines: async (name: string): Promise<QueryResultString> =>
      ((await (remote.sdk.newUserApi(getSystemApi) as {
        getSystemLogsByNameLines: (value: string) => Promise<{ data: QueryResultString }>;
      }).getSystemLogsByNameLines(name)).data),
    getActivityLogEntries: async (limit = 50): Promise<QueryResultActivityLogEntry> =>
      ((await RemotePluginAxiosInstance.instance.get<QueryResultActivityLogEntry>('/System/ActivityLog/Entries', {
        params: { Limit: limit }
      })).data ?? { Items: [] as ActivityLogEntry[] })
  };

  const devicesApi = remote.sdk.newUserApi(getDevicesApi) as {
    getDevices: () => Promise<{ data: QueryResultDevicesDeviceInfo }>;
    getDevicesInfo: (id: string) => Promise<{ data: SettingsDeviceDetails }>;
    getDevicesOptions: (id: string) => Promise<{ data: DevicesDeviceOptions }>;
    postDevicesOptions: (body: DevicesDeviceOptions, id: string) => Promise<unknown>;
    deleteDevice: (payload: { id: string }) => Promise<unknown>;
  };

  const apiKeysApi = {
    getKeys: async (): Promise<AuthenticationInfo[]> =>
      ((await (remote.sdk.newUserApi(getApiKeyApi) as {
        getKeys: () => Promise<{ data: { Items?: AuthenticationInfo[] } }>;
      }).getKeys()).data.Items ?? []),
    createKey: (app: string) =>
      (remote.sdk.newUserApi(getApiKeyApi) as {
        createKey: (payload: { app: string }) => Promise<unknown>;
      }).createKey({ app }),
    revokeKey: (key: string) =>
      (remote.sdk.newUserApi(getApiKeyApi) as {
        revokeKey: (payload: { key: string }) => Promise<unknown>;
      }).revokeKey({ key })
  };

  const sessionsApi = {
    getSessions: async (): Promise<SettingsSessionInfo[]> =>
      ((await RemotePluginAxiosInstance.instance.get<SettingsSessionInfo[]>('/Sessions')).data ?? []),
    sendPlayingCommand: (id: string, command: string) =>
      RemotePluginAxiosInstance.instance.post(`/Sessions/${id}/Playing/${command}`, {
        Name: command,
        Command: command
      }),
    sendMessage: (id: string, payload: { Header?: string; Text?: string; TimeoutMs?: number }) =>
      RemotePluginAxiosInstance.instance.post(`/Sessions/${id}/Message`, payload)
  };

  const serverApi = {
    getLocalizationOptions: async (): Promise<GlobalizationLocalizatonOption[]> =>
      ((await (remote.sdk.newUserApi(getLocalizationApi) as {
        getLocalizationOptions: () => Promise<{ data: GlobalizationLocalizatonOption[] }>;
      }).getLocalizationOptions()).data ?? []),
    getConfiguration: async (): Promise<ServerConfiguration> =>
      ((await (remote.sdk.newUserApi(getConfigurationApi) as {
        getConfiguration: () => Promise<{ data: ServerConfiguration }>;
      }).getConfiguration()).data),
    updateConfiguration: (body: ServerConfiguration) =>
      (remote.sdk.newUserApi(getConfigurationApi) as {
        updateConfiguration: (payload: { serverConfiguration: ServerConfiguration }) => Promise<unknown>;
      }).updateConfiguration({ serverConfiguration: body }),
    updateNamedConfiguration: (key: string, body: unknown) =>
      (remote.sdk.newUserApi(getConfigurationApi) as {
        updateNamedConfiguration: (payload: { key: string; body: string }) => Promise<unknown>;
      }).updateNamedConfiguration({
        key,
        body: typeof body === 'string' ? body : JSON.stringify(body)
      }),
    getBrandingOptions: async (): Promise<BrandingBrandingOptions> =>
      ((await (remote.sdk.newUserApi(getBrandingApi) as {
        getBrandingOptions: () => Promise<{ data: BrandingBrandingOptions }>;
      }).getBrandingOptions()).data),
    getSystemInfo: async (): Promise<SystemInfo> =>
      ((await RemotePluginAxiosInstance.instance.get<SystemInfo>('/System/Info')).data)
  };

  return {
    accountApi,
    logsApi,
    devicesApi,
    apiKeysApi,
    sessionsApi,
    serverApi
  };
}
