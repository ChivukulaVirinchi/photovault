import { call } from "./index";
import type { Page, PhotoDto, PhotoSummaryDto } from "./types";

export const photos = {
  list: (args: {
    cursor?: string | null;
    limit?: number;
    includeTrashed?: boolean;
  } = {}) =>
    call<Page<PhotoSummaryDto>>("photos_list", {
      cursor: args.cursor ?? null,
      limit: args.limit ?? null,
      include_trashed: args.includeTrashed ?? false,
    }),
  get: (id: number) => call<PhotoDto>("photos_get", { id }),
  getMany: (ids: number[]) => call<PhotoDto[]>("photos_get_many", { ids }),
};
