import { apiFetch, type ApiResult } from "./api";

export type Task = {
  id: string;
  doc_id: string;
  doc_title: string;
  item_index: number;
  text: string;
  checked: boolean;
  completed_at: string | null;
  updated_at: string;
};

export const tasksApi = {
  async list(includeCompleted = false): Promise<ApiResult<Task[]>> {
    const qs = includeCompleted ? "?include_completed=true" : "";
    return apiFetch<Task[]>(`/api/workspace/tasks${qs}`);
  },
  async setChecked(docId: string, itemIndex: number, checked: boolean): Promise<ApiResult<void>> {
    return apiFetch<void>(
      `/api/docs/${encodeURIComponent(docId)}/tasks/${itemIndex}`,
      { method: "PATCH", body: { checked } },
    );
  },
};
