import { ApiError } from "./api";
import { t, type Language, type MessageKey } from "./i18n";

const ERROR_KEYS: Record<string, MessageKey> = {
  forbidden_host: "player.error.generic",
  invalid_json: "player.error.request_failed",
  unsupported_media_type: "player.error.request_failed",
  payload_too_large: "player.error.request_failed",
  no_current_track: "player.error.no_current_track",
  player_not_found: "player.error.player_not_found",
  lyrics_pending: "player.error.lyrics_pending",
  lyrics_unavailable: "player.error.lyrics_unavailable",
  lyrics_not_found: "player.error.lyrics_not_found",
  not_supported: "player.error.not_supported",
  player_error: "player.error.player_error",
  track_changed: "player.error.track_changed",
  media_unavailable: "player.error.media_unavailable",
  invalid_value: "player.error.invalid_value",
  invalid_duration: "player.error.invalid_value",
  invalid_source: "player.error.invalid_value",
  missing_position: "player.error.invalid_value",
  missing_delta: "player.error.invalid_value",
  unknown_action: "player.error.request_failed",
  lyrics_too_long: "player.error.lyrics_too_long",
  library_error: "player.error.library_error",
  lyrics_empty: "player.error.lyrics_empty",
  no_original: "player.error.no_original",
  invalid_key: "player.error.invalid_key",
  not_found: "player.error.lyrics_not_found",
  invalid_response: "player.error.request_failed",
  request_failed: "player.error.request_failed",
};

export function errorMessage(language: Language, error: unknown): string {
  if (error instanceof ApiError) {
    const key = ERROR_KEYS[error.code];
    if (key) {
      return t(language, key);
    }
  }
  return t(language, "player.error.generic");
}

export function sourceLabelKey(source: string): MessageKey {
  const keys: Record<string, MessageKey> = {
    lrclib: "library.source.lrclib",
    manual: "library.source.manual",
    tap: "library.source.tap",
  };
  return keys[source] ?? "library.source.unknown";
}
