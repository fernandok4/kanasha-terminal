# Scroll do Codex no KanashaTerminal

## Atualização: gatilho confirmado ao dividir painéis

O usuário identificou que o problema aparece ao abrir outro terminal pelo botão
de divisão. A estrutura React recursiva desmontava o TerminalPanel existente ao
transformar uma folha em divisão, ou colapsar uma divisão ao fechar um vizinho.
O PTY continuava vivo, mas o novo xterm perdia os modos de mouse e seu buffer.

O layout agora calcula posições a partir da árvore e mantém todos os terminais
como irmãos com chaves e ordem estáveis. Dividir, fechar vizinhos e maximizar
alteram somente posição, tamanho e visibilidade; não recriam os outros terminais.
Divisórias continuam permitindo ajuste por mouse e teclado.

Validação desta correção:

- Os dois testes de regressão (dividir à direita/abaixo e fechar o vizinho)
  falharam antes porque o elemento do terminal original era substituído.
  Após a correção, passaram todos os 23 testes de App/TerminalPanel.
- Interface App + TerminalPanel + xterm reais em Chromium, simulando o transporte
  Tauri e a saída ANSI: com a estrutura recursiva anterior, o gesto passava de
  três eventos de mouse para zero após dividir à direita, e o xterm era destruído.
- Com a correção, a instância, o buffer normal e o modo alternativo persistem
  durante divisão à direita, divisão aninhada abaixo, ajuste de divisórias por
  teclado e arraste, maximização/restauração e fechamento dos dois novos painéis.
  Cada gesto simulado continua gerando três ou quatro eventos de mouse.
- Verificados também tamanhos positivos e ausência de sobreposição dos painéis.
- Regressão de navegador preservada em `split-regression.cjs`. Ela usa
  `playwright-core` externo, resolvido pelo `NODE_PATH`, e aceita o caminho do
  navegador por `CHROMIUM_EXECUTABLE`; não inicia sessões reais do Codex.

Ainda não há validação com trackpad físico no WKWebView. Terminais cujo estado
já foi perdido precisam ser reabertos/retomados na versão atualizada.

## Diagnóstico anterior: saída inicial

## Relato

Gesto de dois dedos no trackpad deixa de rolar a conversa, sem barra de rolagem.
Possível associação com troca de abas ou Área de Trabalho.

## Falha reproduzida e corrigida

O backend inicia o PTY antes de devolver o snapshot contendo `activeTerminalIds`.
O painel já ouve `terminal-output`, mas só cria o xterm quando recebe `active=true`.
A implementação anterior descartava os eventos recebidos nessa janela.
Esses eventos incluem sequências que habilitam tela alternativa e captura do mouse.

Uma execução local isolada do Codex 0.158.0, sem enviar prompt, confirmou a emissão
de DECSET 1049, 1000, 1002, 1006 e 1003 na inicialização. Comandos desse tipo perdidos
podem deixar o conteúdo visível sem encaminhar gestos ao programa.

`TerminalPanel.tsx` agora guarda a saída recebida antes de existir o xterm e a entrega
na ordem original, depois de conectar o caminho de entrada ao PTY. Preserva inclusive
sequências divididas entre eventos. Essa fila é transitória em memória e é limpa ao
consumir os eventos ou desmontar o painel.

## Validação

- Regressão React: emitir a sequência de mouse antes de `active=true` falhava antes
  da alteração e passou depois. Confere ordem, fragmentação e ausência de duplicação.
- 20 testes de TerminalPanel e App passaram; TypeScript e Vite passaram.
- Chromium local com xterm 6 real, gesto representado por 20 eventos de 8 pixels:
  sem comandos iniciais, zero eventos de mouse encaminhados; com os comandos,
  três eventos encaminhados (o xterm acumula os pequenos deslocamentos do trackpad).
- Componente React real + xterm, transporte Tauri simulado: três eventos ao iniciar
  e três após cinco ciclos de ocultar/exibir, recebendo saída enquanto oculto.
- Abertura inicialmente oculta, seguida de exibição e ajuste de tamanho: eventos
  também encaminhados normalmente.

## Limite do diagnóstico

A troca de abas sozinha não reproduziu o travamento. A perda na inicialização é uma
causa comprovada capaz de produzir o sintoma; não há evidência suficiente para afirmar
que seja a única causa do relato. Os testes de navegador usaram Chromium e eventos
sintéticos, não o trackpad físico no WKWebView do macOS.

A tela alternativa não oferece o scrollback nativo do terminal. O Codex deve receber
o gesto e rolar sua própria conversa; ausência da barra nativa isoladamente não
comprova falha. O modo alternativo é configurável conforme a documentação oficial:
https://learn.chatgpt.com/docs/config-file/config-reference (tui.alternate_screen).
