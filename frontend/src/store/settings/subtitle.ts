/**
 * Subtitle settings store placeholder
 */

import { reactive } from 'vue';

const state = reactive({
  enabled: true,
  stroke: false,
  fontFamily: 'default',
  fontSize: 1,
  positionFromBottom: 10,
  backdrop: false
});

export const subtitleSettings = {
  state
};