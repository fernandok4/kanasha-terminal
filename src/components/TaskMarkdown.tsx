import { useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";

function externalUrl(value: string): string | undefined {
  try {
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol) ? url.href : undefined;
  } catch {
    return undefined;
  }
}

export function TaskMarkdown({ content }: { content: string }) {
  const [linkError, setLinkError] = useState(false);
  return <div className="task-markdown">
    <Markdown remarkPlugins={[remarkGfm]} skipHtml disallowedElements={["img"]} urlTransform={externalUrl} components={{
      h1: ({ children }) => <h2>{children}</h2>,
      a: ({ href, children }) => href ? <a href={href} title={href} onClick={(event) => {
        event.preventDefault();
        setLinkError(false);
        void openUrl(href).catch(() => setLinkError(true));
      }}>{children}</a> : <span>{children}</span>,
      table: ({ children }) => <div className="task-markdown-table"><table>{children}</table></div>,
    }}>{content}</Markdown>
    {linkError ? <p role="alert">Não foi possível abrir o link. Copie o endereço pelo menu do link e abra no navegador.</p> : null}
  </div>;
}
