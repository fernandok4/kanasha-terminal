# KanashaTerminal

Aplicativo desktop local para organizar agentes de terminal em paralelo. A interface está em PT-BR e funciona em Linux e macOS por meio de Tauri, Rust e WebView; a validação executada neste repositório cobre Linux.

## O que o MVP oferece

- Áreas de Trabalho vinculadas a uma pasta-raiz.
- Workspaces em abas dentro de cada Área.
- Vários terminais em painéis com divisões horizontais ou verticais.
- Divisores arrastáveis e ajustáveis por teclado, com proporções restauradas localmente.
- Foco rápido entre painéis, maximização temporária, nomes e etiquetas visíveis para identificar cada agente.
- Perfis internos para Shell, Codex, Claude e Gemini, quando disponíveis no `PATH`, além de perfis customizados.
- Processos que continuam ativos ao trocar de Área ou Workspace.
- Restauração local de Áreas, Workspaces, layouts, painéis e metadados de perfis. Terminais restaurados ficam parados e podem ser iniciados manualmente.
- Menus de contexto, atalhos de teclado, tooltips e confirmação antes de encerrar processos.

O terminal sempre inicia na pasta-raiz da Área. Os dados ficam somente no diretório de configuração do aplicativo e não há servidor HTTP, WebSocket ou porta de controle.

## Privacidade da persistência

O aplicativo **não persiste** buffers ou saída de terminal, comandos executados, argumentos de perfil, tokens, segredos, variáveis de ambiente, PIDs ou sessões. Para proteger essa fronteira, o executável e os argumentos de um perfil customizado são mantidos somente em memória: devem ser informados novamente após reiniciar o aplicativo.

## Desenvolvimento

Pré-requisitos no Linux: Rust, Node.js, bibliotecas de desenvolvimento WebKitGTK/GTK e as dependências de sistema do Tauri.

```bash
npm install
npm run tauri dev
```

## Validação

```bash
npm test
npm run build
cd src-tauri && cargo test
npm run tauri build
```

O último comando gera, localmente, pacotes para Linux em `src-tauri/target/release/bundle/`. Ele não publica, assina ou faz deploy.

## Atalhos

- `Ctrl/Cmd + N`: cria um novo terminal no workspace atual.
- `Ctrl/Cmd + Shift + N`: abre outro terminal mesmo quando um terminal está com foco.
- `Ctrl/Cmd + Shift + W`: cria um novo workspace na Área atual.
- `Ctrl/Cmd + ,`: abre a criação de perfil customizado.
- `Ctrl/Cmd + Shift + A`: abre a criação de Área de Trabalho.
- `Ctrl/Cmd + Alt + ←/↑` e `Ctrl/Cmd + Alt + →/↓`: muda o foco para o painel anterior/próximo, inclusive a partir do terminal.
- `Ctrl/Cmd + Shift + C` e `Ctrl/Cmd + Shift + V`: copiam a seleção e colam texto no terminal, pelos controles nativos do xterm/WebView.

Os atalhos são locais à janela. Os de criação não interceptam a entrada do terminal quando o painel está com foco; a navegação entre painéis foi deliberadamente mantida disponível nesse contexto.
