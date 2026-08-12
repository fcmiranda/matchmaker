# Avaliação: daemon nativo para o Matchmaker

## Veredito

Vale a pena, mas não na forma proposta em
[`PROMPT_DAEMON_IMPLEMENTATION.md`](../PROMPT_DAEMON_IMPLEMENTATION.md).

Um daemon é uma otimização justificável e potencialmente muito útil para o fluxo de
navegação de arquivos (`jump`). O ganho real é eliminar o custo recorrente de executar
`fd`/`find`, percorrer o filesystem e reinjetar itens a cada entrada ou reload. Não deve,
porém, substituir o motor geral do `matchmaker-lib` nem ser aplicado a todas as fontes de
dados.

A meta de “TUI visível em menos de 2 ms” não é um critério de aceite realista para o
processo inteiro: mesmo com índice quente há `exec`, inicialização do runtime, entrada em
raw/alternate screen, primeira renderização e I/O do terminal. As métricas devem separar
conexão e leitura do índice, disponibilidade de itens para o primeiro frame, first paint,
latência query-resultados e consumo de RAM/CPU em idle.

## Leitura da arquitetura atual

O projeto separa bem o picker genérico (`matchmaker-lib`) da política da CLI:

- `nucleo::Worker` mantém os snapshots, matching incremental, colunas, highlights e a
  ordenação; o renderer depende diretamente desse modelo.
- A frecency já é carregada uma vez como snapshot em memória ao construir o matcher, fora
  do loop crítico.
- A CLI possui um cache especulativo de diretórios, limitado a 64 entradas; ele antecipa
  o reload, mas ainda roda o comando configurado (`fd`/`find`) em subprocessos.
- O preset `jump` é a principal evidência de necessidade: ele executa `fd` por diretório,
  possui múltiplas fontes e regras explícitas de exclusão.

Referências internas:

- [`matchmaker-lib/ARCHITECTURE.md`](../matchmaker-lib/ARCHITECTURE.md)
- [`Worker`](../matchmaker-lib/src/nucleo/worker.rs)
- [carregamento da frecency](../matchmaker-lib/src/matchmaker.rs)
- [cache especulativo e reload](../matchmaker-cli/src/start.rs)
- [preset `jump`](../matchmaker-cli/assets/presets/jump.toml)

## Principal problema da especificação

O trait `SearchEngine` proposto abstrai no nível errado. Substituir o `Worker<nucleo>` por
um `DaemonIpcEngine::filter()` obriga o daemon a reproduzir funcionalidades atualmente
integradas ao picker:

- matching incremental e highlights por coluna;
- snapshots, cursor, seleção múltipla e `raw_results`;
- ranking de nucleo, frecency, depth penalty e `dir_first`;
- streaming enquanto itens são carregados;
- consistência entre item exibido, aceito e enviado ao preview/template.

Além disso, `fn filter(&self) -> Vec<RenderItem>` é síncrono: uma leitura de socket no
event/render loop pode travar a TUI. Um `limit` de resultados também quebra scroll,
seleção e contagens corretas. O `RenderItem` sugerido não carrega colunas, ANSI, grupos e
outros dados usados pelo picker.

### Direção recomendada

Manter `nucleo` e `Worker` no cliente. O daemon deve fornecer um snapshot de candidatos
para uma raiz de diretório, isto é, operar como `ItemSource` ou `IndexProvider`. A CLI
injeta esse snapshot no pipeline atual. Isso preserva matching, renderização, preview,
templates, seleção e compatibilidade com presets/pipes.

## Escopo recomendado

O primeiro produto deve ser explícito e opt-in, por exemplo `mm daemon index <root>` e
uma fonte/configuração `daemon`. Não deve haver auto-spawn no primeiro MVP.

O daemon deve indexar apenas uma fonte semanticamente definida: a árvore de arquivos de
uma raiz, com regras de inclusão versionadas. O Matchmaker suporta comandos arbitrários
para `rg`, Git, Docker, SSH, pipes e scripts; eles não são equivalentes a um índice de
filesystem e devem continuar no caminho atual.

Cada índice deve ser identificado por algo como:

```text
canonical_root + regras_de_ignore + include_hidden + follow_symlinks + schema_version
```

Evitar um índice global de `$HOME`, que é caro, pouco previsível e pode indexar paths fora
da intenção do usuário.

## IPC e segurança

“Zero-copy sobre Unix socket” é impreciso: AF_UNIX ainda copia bytes entre processos e o
kernel. `rkyv` pode eliminar parte da desserialização/cópias posteriores, mas o cliente
ainda construirá itens locais para o picker. A escolha de serialização deve vir depois de
um benchmark contra `postcard` ou `bincode`.

O protocolo deve ter framing por tamanho, `version`, `request_id`, `generation`, raiz,
limites de frame e timeout. O cliente deve descartar respostas de gerações antigas; assim,
uma resposta de uma query antiga não sobrescreve a tela atual.

Se usar `rkyv`:

- fixar explicitamente a versão do protocolo e da crate;
- validar bytes recebidos; acesso sem validação pressupõe bytes confiáveis;
- não usar `unsafe` para economizar microssegundos antes de medir o gargalo.

Não usar `/tmp/matchmaker-$UID.sock`. Preferir
`$XDG_RUNTIME_DIR/matchmaker/...`, com diretório `0700`, lock de instância, permissões
verificadas e tratamento seguro de socket stale. `$XDG_RUNTIME_DIR` é o local definido
para sockets e objetos de runtime por usuário.

Também faltam na proposta: validação do peer local, máximo de clientes, backpressure,
shutdown, socket stale, upgrade de binário e múltiplas sessões/raízes/configurações.

## Filesystem e consistência

Respeitar `.gitignore`, `.ignore`, arquivos ocultos e exclusões não é responsabilidade do
watcher. `notify` informa mudanças; o indexador precisa aplicar regras equivalentes a
`fd`/`ignore`. Uma alteração em um arquivo de ignore pode exigir reindexar uma subárvore.

Definir e testar explicitamente:

- symlinks, ciclos e follow-symlinks;
- mounts, permission denied e arquivos que desaparecem durante a operação;
- renames atômicos de editores e Git;
- overflow/coalescing do watcher e rescan periódico/completo;
- limites de RAM e eviction de várias raízes;
- persistência do índice versus reconstrução;
- paths não UTF-8 em Unix.

A frecency deve continuar como estado durável separado do índice. Centralizá-la no daemon
pode reduzir contenção no `redb`, mas exige fila durável ou confirmação para `RecordAccess`.

## Roadmap aprovado

1. Instrumentar o fluxo atual em 10k, 100k e 600k arquivos: comando, parse/injeção,
   first paint e query.
2. Extrair somente uma interface de fonte de itens na CLI, preservando o `Worker` atual.
3. Implementar cache local persistente por raiz e medir o ganho sem processo extra.
4. Transformar esse cache em daemon apenas se startup/reloads permanecerem o gargalo.
5. Adicionar watcher, IPC e auto-discovery atrás de feature experimental/opt-in.
6. Considerar query remota somente se os benchmarks provarem que o nucleo local é o
   gargalo; isso é improvável para o primeiro objetivo.

O subcomando `mm cache` existente é um ponto de partida, mas atualmente escreve um único
arquivo e não é consumido no fluxo de start; também implementa uma caminhada com regras de
exclusão incompletas. Ele deve ser evoluído ou substituído antes da criação da crate daemon.

## Referências externas

- [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir/)
- [documentação de `notify-debouncer-full`](https://docs.rs/notify-debouncer-full/latest/notify_debouncer_full/)
- [API de acesso e validação de `rkyv`](https://docs.rs/rkyv/latest/rkyv/api/)
