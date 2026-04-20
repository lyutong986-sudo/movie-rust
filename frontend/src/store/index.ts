import { shallowRef } from 'vue';
import { useMediaControls } from '@vueuse/core';

export const mediaElementRef = shallowRef<HTMLMediaElement>();
export const mediaControls = useMediaControls(mediaElementRef);
export const mediaWebAudio = {
  context: shallowRef<AudioContext>(),
  sourceNode: shallowRef<MediaElementAudioSourceNode>()
};

export { playbackManager } from './playback-manager';
export { playerElement } from './player-element';