import { ChevronRight } from "lucide-react";
import { Link } from "react-router-dom";

export function Breadcrumb({ items }: { items: Array<{ id?: string; title: string }> }) {
  return (
    <nav aria-label="Breadcrumb" className="text-[12px] text-fg-muted flex items-center flex-wrap">
      {items.map((it, i) => (
        <span key={i} className="inline-flex items-center">
          {i > 0 && <ChevronRight size={12} aria-hidden className="mx-1 opacity-60" />}
          {it.id ? (
            <Link to={`/doc/${it.id}`} className="hover:text-fg transition-colors">
              {it.title}
            </Link>
          ) : (
            <span>{it.title}</span>
          )}
        </span>
      ))}
    </nav>
  );
}
