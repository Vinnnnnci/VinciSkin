import { AlertBox, Button, EmptyState, Spinner, useConfirm, useToast } from "@/components";
import { describeError, errorMessage, errorSummary, m } from "@/i18n";
import {
  api,
  isOk,
  type ChampionSwap,
  type CnHero,
  type CnQuality,
  type CnSkin,
} from "@/lib/tauri";

import {
  useApplyCnSkin,
  useCnHeroSkins,
  useForgetCnSwap,
  useHeroSwap,
  useRestoreCnSkin,
} from "../api";
import { type SkinCard, filterCards, qualityLabel, skinCards } from "../skins";

interface CnSkinGridProps {
  /** The champion to list, or none until one is picked. */
  hero: CnHero | null;
  /** The tier the page narrowed to, or none. */
  quality: CnQuality | null;
  /** Whether only the skin the champion shows is listed. */
  onlyApplied: boolean;
}

/** One champion's skins, read from the table the Chinese client itself ships. */
export function CnSkinGrid({ hero, quality, onlyApplied }: CnSkinGridProps) {
  const skins = useCnHeroSkins(hero?.id ?? null);
  const swap = useHeroSwap(hero?.alias ?? null);
  const apply = useApplyCnSkin();
  const restore = useRestoreCnSkin();
  const forget = useForgetCnSwap();
  const toast = useToast();
  const confirm = useConfirm();

  /* One write serves every card, so a card pressed while another is saving is
     held rather than starting a second one. */
  const pending = apply.isPending || restore.isPending || forget.isPending;

  /* A record the archive no longer agrees with refuses both a write and a
     restore, so the cards stay shut until the record is let go of. */
  const blocked = swap?.state === "drifted" || swap?.state === "backupMissing";

  async function announceApply(name: string) {
    const status = await api.cn.patcherStatus();

    if (!isOk(status)) {
      toast.warning(
        m.cn_skin_applied_injection_off_title(),
        m.cn_skin_applied_injection_off_description(),
      );
      return;
    }

    if (status.value.error) {
      toast.error(m.cn_inject_failed_title(), status.value.error);
      return;
    }

    if (status.value.gameRunning) {
      toast.warning(
        m.cn_skin_applied_game_running_title(),
        m.cn_skin_applied_game_running_description(),
      );
      return;
    }

    if (!status.value.running) {
      toast.toast({
        title: m.cn_skin_applied_injection_off_title(),
        description: m.cn_skin_applied_injection_off_description(),
        type: "warning",
        timeout: 8000,
        action: {
          label: m.cn_inject_start_action(),
          onClick: () => {
            void api.cn.startPatcher().then((result) => {
              if (!isOk(result)) {
                toast.error(m.cn_inject_failed_title(), errorMessage(result.error));
              }
            });
          },
        },
      });
      return;
    }

    toast.success(m.cn_skin_applied({ name }));
  }

  async function pick(picked: CnSkin) {
    if (hero === null) return;

    /* The base skin's card asks for the archive back rather than a write, which
       is the backend's own restore path. */
    if (picked.isBase) {
      restore.mutate(
        { alias: hero.alias },
        {
          onSuccess: () => toast.success(m.cn_skin_reset()),
          onError: (error) => toast.error(m.cn_restore_skin_failed_title(), errorSummary(error)),
        },
      );
      return;
    }

    /* Writing into the game's own files is worth a confirmation every time
       rather than only the first. */
    const accepted = await confirm({
      title: m.cn_apply_skin_confirm_title({ name: picked.name }),
      description: m.cn_apply_skin_confirm_description(),
      confirmLabel: m.cn_apply_skin_action(),
      tone: "warning",
    });
    if (!accepted) return;

    apply.mutate(
      { alias: hero.alias, heroId: picked.heroId, skinId: picked.skinId },
      {
        onSuccess: () => {
          void announceApply(picked.name);
        },
        onError: (error) => toast.error(m.cn_apply_skin_failed_title(), errorSummary(error)),
      },
    );
  }

  async function drop() {
    if (hero === null) return;

    const accepted = await confirm({
      title: m.cn_swap_forget_confirm_title(),
      description: m.cn_swap_forget_confirm_description(),
      confirmLabel: m.cn_swap_forget_action(),
      tone: "warning",
    });
    if (!accepted) return;

    forget.mutate(
      { alias: hero.alias },
      {
        onSuccess: () => toast.success(m.cn_swap_forgotten()),
        onError: (error) => toast.error(m.cn_swap_forget_failed_title(), errorSummary(error)),
      },
    );
  }

  if (hero === null) {
    return <EmptyState size="sm" title={m.cn_select_hero_hint()} />;
  }

  if (skins.isPending) {
    return (
      <div className="flex items-center justify-center gap-2 py-8 text-meta text-surface-400">
        <Spinner size="sm" />
        {m.cn_skins_loading()}
      </div>
    );
  }

  if (skins.isError) {
    const copy = describeError(skins.error);

    return (
      <EmptyState
        size="sm"
        title={m.cn_skins_error_title()}
        description={copy.detail ?? copy.description}
      />
    );
  }

  if (skins.data.skins.length === 0) {
    return <EmptyState size="sm" title={m.cn_skins_empty()} />;
  }

  /* The record holds the game's own skin number, so the catalog's id is the
     hero's thousand back on top of it. Only an applied record counts: a reverted
     one is the client's own file again. */
  const appliedSkinId =
    swap?.state === "applied" && swap.skin !== null ? hero.id * 1000 + swap.skin : null;

  /* What the champion was last given, which outlives the archive being put back: the page offers
     the same skin again rather than making the reader find it in the table. */
  const lastSkinId =
    appliedSkinId === null && swap?.lastSkin != null ? hero.id * 1000 + swap.lastSkin : null;

  const cards = filterCards(skinCards(skins.data.skins), {
    quality,
    onlyApplied,
    appliedSkinId,
  });

  return (
    <div className="flex flex-col gap-3">
      <SwapNotice swap={swap} pending={pending} onForget={drop} />

      {cards.length === 0 ? (
        <EmptyState size="sm" title={m.cn_skins_filtered_empty()} />
      ) : (
        <ul className="grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-4">
          {cards.map((card) => (
            <SkinCardItem
              key={card.skin.skinId}
              card={card}
              appliedSkinId={appliedSkinId}
              lastSkinId={lastSkinId}
              pending={pending || blocked}
              onPick={pick}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

interface SwapNoticeProps {
  swap: ChampionSwap | null;
  pending: boolean;
  onForget: () => void;
}

/** What the record says about a champion whose archive is not showing a swap. */
function SwapNotice({ swap, pending, onForget }: SwapNoticeProps) {
  /* Nothing to say while the archive is what we wrote, or while there is no record at all: a
     champion that is only remembered is the ordinary case, and its memory speaks for itself on
     the card. */
  if (swap === null || swap.state === "applied" || swap.state === "untouched") return null;

  /* A record the archive no longer agrees with is the one state that needs an action rather than
     a word: neither a write nor a restore is safe until it is let go of. */
  const needsForgetting = swap.state === "drifted" || swap.state === "backupMissing";
  const text =
    swap.state === "drifted"
      ? m.cn_swap_drifted_notice()
      : swap.state === "backupMissing"
        ? m.cn_swap_backup_missing_notice()
        : m.cn_swap_reverted_notice();

  return (
    <div className="flex flex-wrap items-center gap-3" data-ui="cn-swap-notice">
      <AlertBox variant="warning" className="flex-1">
        {text}
      </AlertBox>
      {needsForgetting && (
        <Button size="xs" variant="outline" compact disabled={pending} onClick={onForget}>
          {m.cn_swap_forget_action()}
        </Button>
      )}
    </div>
  );
}

interface SkinCardItemProps {
  card: SkinCard;
  appliedSkinId: number | null;
  /** The skin the champion was last given, where nothing is written now. */
  lastSkinId: number | null;
  pending: boolean;
  onPick: (skin: CnSkin) => void;
}

function SkinCardItem({ card, appliedSkinId, lastSkinId, pending, onPick }: SkinCardItemProps) {
  const { skin, chromas } = card;

  const chromaOf = (skinId: number | null) =>
    skinId === null ? undefined : chromas.find((chroma) => chroma.skinId === skinId);

  const appliedChroma = chromaOf(appliedSkinId);
  const rememberedChroma = chromaOf(lastSkinId);
  const isApplied = skin.skinId === appliedSkinId || appliedChroma !== undefined;
  /* Remembered is only worth saying where nothing is written: an applied card already says so,
     and the two are the same skin on an archive that was never put back. */
  const isRemembered = !isApplied && (skin.skinId === lastSkinId || rememberedChroma !== undefined);

  /* A chroma is shown in place of the skin it varies, so the card looks like what the champion
     would actually wear - what it holds, or what it was last given. */
  const shown = appliedChroma ?? rememberedChroma ?? skin;

  return (
    <li className="flex flex-col gap-2 rounded-lg border border-surface-700 bg-surface-800 p-2">
      <div className="aspect-video w-full overflow-hidden rounded-md bg-surface-900">
        {shown.artUrl && (
          <img src={shown.artUrl} alt="" loading="lazy" className="h-full w-full object-cover" />
        )}
      </div>

      <span className="truncate text-meta text-surface-200" title={shown.name}>
        {shown.name}
      </span>

      <div className="flex flex-wrap items-center gap-x-2">
        {skin.isBase ? (
          <span className="text-fine text-surface-500">{m.cn_base_skin_label()}</span>
        ) : (
          <span
            data-ui="cn-quality-badge"
            className="rounded-sm border border-surface-600 px-1 text-fine text-surface-400"
          >
            {qualityLabel(skin.quality)}
          </span>
        )}
        {skin.emblemsName && !skin.isBase && (
          <span className="truncate text-fine text-surface-500">{skin.emblemsName}</span>
        )}
        {isApplied && (
          <span data-ui="cn-applied-badge" className="text-fine text-accent-400">
            {m.cn_applied_skin_label()}
          </span>
        )}
        {isRemembered && (
          <span data-ui="cn-remembered-badge" className="text-fine text-surface-500">
            {m.cn_last_used_skin_label()}
          </span>
        )}
      </div>

      {chromas.length > 0 && (
        <ul
          className="flex flex-wrap gap-1"
          aria-label={m.cn_chroma_count({ count: chromas.length })}
        >
          {chromas.map((chroma) => (
            <li key={chroma.skinId}>
              <button
                type="button"
                title={m.cn_chroma_action({ name: chroma.name })}
                aria-label={m.cn_chroma_action({ name: chroma.name })}
                disabled={pending}
                onClick={() => onPick(chroma)}
                className={
                  chroma.skinId === appliedSkinId
                    ? "h-8 w-8 overflow-hidden rounded border-2 border-accent-400"
                    : "h-8 w-8 overflow-hidden rounded border border-surface-600 hover:border-accent-400"
                }
              >
                {chroma.artUrl ? (
                  <img
                    src={chroma.artUrl}
                    alt=""
                    loading="lazy"
                    className="h-full w-full object-cover"
                  />
                ) : (
                  <span className="block h-full w-full bg-surface-700" />
                )}
              </button>
            </li>
          ))}
        </ul>
      )}

      <Button
        size="xs"
        variant={isApplied ? "ghost" : "outline"}
        compact
        /* The base skin's own card is the way back, so it is only offered once
           there is something to go back from. */
        disabled={pending || isApplied || (skin.isBase && appliedSkinId === null)}
        onClick={() => onPick(skin)}
      >
        {skin.isBase ? m.cn_reset_skin_action() : m.cn_apply_skin_action()}
      </Button>
    </li>
  );
}
