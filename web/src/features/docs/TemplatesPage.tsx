/**
 * /templates — browse and edit workspace templates.
 *
 * Templates are docs flagged with `is_template = true`. They're filtered
 * out of the main tree, but clicking a card here navigates to the
 * underlying DocPage where the user can edit the content normally and
 * unmark it as a template from the page's header.
 */

import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { LayoutTemplate } from "lucide-react";

import { docsApi } from "./docs.api";

export default function TemplatesPage() {
  const templates = useQuery({
    queryKey: ["templates"],
    queryFn: () => docsApi.listTemplates(),
    refetchOnMount: "always",
    staleTime: 0,
  });

  const items = templates.data && "ok" in templates.data ? templates.data.ok : [];

  return (
    <section className="mx-auto max-w-[760px] px-6 py-8" data-testid="templates-page">
      <header className="mb-6">
        <h1 className="text-2xl font-bold text-fg">Templates</h1>
        <p className="text-sm text-fg-muted mt-1">
          Click a template to edit its content or unmark it from the document header.
          Start a new document from a template via the <span className="font-medium text-fg">+</span>{" "}
          button in the sidebar.
        </p>
      </header>

      {templates.isLoading && (
        <p className="text-fg-muted text-sm" data-testid="templates-loading">
          Loading…
        </p>
      )}

      {!templates.isLoading && items.length === 0 && (
        <div className="bg-surface border border-border rounded-lg px-6 py-10 text-center">
          <LayoutTemplate size={28} className="mx-auto text-fg-muted mb-3" aria-hidden />
          <p className="text-sm text-fg-muted m-0">
            No templates yet. Open a doc and choose “Save as template” from its header to create one.
          </p>
        </div>
      )}

      {!templates.isLoading && items.length > 0 && (
        <ul className="grid grid-cols-1 sm:grid-cols-2 gap-3 list-none m-0 p-0" data-testid="templates-list">
          {items.map((t) => (
            <li key={t.id}>
              <Link
                to={`/doc/${t.id}`}
                data-testid={`template-${t.id}`}
                className="block h-full bg-surface border border-border rounded-lg p-4 hover:bg-muted transition-colors no-underline"
              >
                <div className="flex items-start gap-3">
                  <LayoutTemplate size={18} className="text-fg-muted mt-0.5 shrink-0" aria-hidden />
                  <div className="min-w-0">
                    <div className="text-sm font-medium text-fg truncate">{t.title}</div>
                    <div className="text-xs text-fg-muted mt-1">Click to edit</div>
                  </div>
                </div>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
