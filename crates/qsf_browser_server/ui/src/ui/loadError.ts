import type { LoadError } from "../types";
import { escapeHtml } from "./html";

export function renderLoadError(root: HTMLElement, err: LoadError) {
  root.className = "";
  root.innerHTML = `
    <div class="load-error">
      <h2>memory store failed to load</h2>
      <p><strong>kind:</strong> <code>${escapeHtml(err.kind)}</code></p>
      <p><strong>path:</strong> <code>${escapeHtml(err.path)}</code></p>
      <p><strong>message:</strong> ${escapeHtml(err.message)}</p>
      ${"schema_versions_found" in err ? `<p><strong>schema versions found:</strong><br />records: <code>${err.schema_versions_found.records.join(", ") || "(none)"}</code><br />associations: <code>${err.schema_versions_found.associations.join(", ") || "(none)"}</code></p>` : ""}
      ${"schema_versions_supported" in err ? `<p><strong>schema versions supported:</strong><br />records: <code>${err.schema_versions_supported.records.join(", ")}</code><br />associations: <code>${err.schema_versions_supported.associations.join(", ")}</code></p>` : ""}
      ${"duplicate_ids" in err ? `<p><strong>duplicate ids:</strong><br /><code>${err.duplicate_ids.map(escapeHtml).join(", ")}</code></p>` : ""}
      ${"shape_errors" in err ? `<p><strong>shape errors:</strong><br />${err.shape_errors.map((e) => `<code>${escapeHtml(e.field_path)}</code>: ${escapeHtml(e.message)}`).join("<br />")}</p>` : ""}
    </div>
  `;
}
