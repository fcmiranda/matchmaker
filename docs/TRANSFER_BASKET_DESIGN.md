# Cesta de Transferência para o Navegador de Arquivos

## Problema

Copiar ou mover itens localizados em ramos diferentes da árvore de arquivos é
lento quando cada seleção obriga o usuário a voltar até um destino distante.
O fluxo atual de `y`/`x`/`p` já permite a operação básica, mas não explicita
uma coleção persistente e revisável de origens.

## Proposta

Adicionar uma **cesta de transferência** (*transfer basket*): uma coleção
persistente, durante a sessão, de caminhos absolutos e da intenção de
transferência (`copy` ou `move`).

O fluxo passa a ser:

```text
Adicionar à cesta → continuar navegando e acumulando itens
→ revisar/remover itens da cesta → escolher ou fixar destino
→ copiar/mover em uma única operação
```

Isso reduz a distância de navegação e torna explícito o estado que será
afetado antes da operação destrutiva de mover.

## Interação sugerida

| Ação | Resultado |
| --- | --- |
| `y` | Adiciona o item ou a seleção à cesta em modo cópia. |
| `x` | Adiciona o item ou a seleção à cesta em modo movimento. |
| `b` | Abre a cesta: lista, origem, modo e quantidade de itens. |
| `d` na cesta | Remove o item da cesta, sem alterar o arquivo de origem. |
| `P` | Cola no diretório atual ou no destino fixado. |
| `D` | Fixa/desafixa o diretório atual como destino da transferência. |

O footer deve expor a contagem e o modo, por exemplo:

```text
[basket: 12 itens, copiar]  destino: ~/Downloads/triagem
```

Antes de um `move`, a visualização da cesta deve deixar claro que os arquivos
de origem serão removidos somente depois de uma transferência bem-sucedida.

## Hierarquia e conflitos

O padrão deve ser **preservar a hierarquia relativa** a uma raiz de origem
definida. Isso evita colisões comuns, como vários arquivos chamados
`config.toml` em diretórios distintos.

Para cada transferência, oferecer:

- preservar hierarquia (padrão);
- achatar no destino, com resolução explícita de conflitos;
- conflito por item: renomear, substituir, ignorar ou cancelar;
- prévia da estrutura de destino antes da confirmação.

Arquivos devem ser guardados como caminhos absolutos na cesta; a raiz relativa
é usada apenas para montar o caminho no destino. Itens inexistentes no momento
da execução devem ser reportados e ignorados, sem abortar silenciosamente os
demais.

## Referências de UX

O [broot](https://dystroy.org/broot/panels/) oferece dois painéis e os comandos
`copy_to_panel`/`move_to_panel`: é uma boa referência para manter um destino
distante visível e executar transferências entre painéis. Ele representa a
seleção atual, não uma cesta acumulada entre várias navegações.

O [Vifm](https://vifm.info/manual.shtml) é a referência para a coleta: seus
*registers* nomeados podem receber itens por adição e serem colados depois.
A cesta proposta combina a persistência dessa lista com o destino fixado dos
painéis do broot.

Para operações em lote fora da TUI, o equivalente robusto é uma lista de
arquivos para `rsync --files-from`, que preserva caminhos relativos e permite
uma validação prévia com `--dry-run`.

## Critérios de aceite

- O usuário pode adicionar arquivos de pelo menos três diretórios não
  relacionados sem perder a seleção anterior.
- O destino pode permanecer fixado enquanto a origem muda.
- A cesta pode ser revisada e editada antes de qualquer cópia ou movimento.
- A cópia preserva a hierarquia por padrão e apresenta conflitos de forma
  determinística.
- Uma falha em um item é comunicada sem ocultar o estado dos demais itens.
