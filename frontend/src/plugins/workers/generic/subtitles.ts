/**
 * Subtitle types placeholder
 */

export interface ParsedSubtitleTrack {
  cues: any[];
  metadata: Record<string, any>;
  sub?: {
    text: string;
  };
}

export interface Dialogue {
  text: string;
  start: number;
  end: number;
}