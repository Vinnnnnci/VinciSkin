import { useEffect, useState } from "react";

import { Button, ComboboxField, EmptyState, Spinner, Switch, useToast } from "@/components";
import { describeError, errorMessage, m } from "@/i18n";
import { api, type CnHero, type CnQuality, isOk, unwrap } from "@/lib/tauri";
import { usePlay } from "@/modules/launcher";

import { useCnHeroes } from "../api";
import { qualityOptions } from "../skins";
import { CnHeroPicker } from "./CnHeroPicker";
import { CnSessionControl } from "./CnSessionControl";
import { CnSkinGrid } from "./CnSkinGrid";
import { CnSkinPacks } from "./CnSkinPacks";

/** The champion and skin tables the Chinese client ships, read from the Tencent CDN. */
export function CnCatalog() {
  const heroes = useCnHeroes();
  const { launchOnly, isBusy } = usePlay();
  const toast = useToast();

  /* Held as the option value rather than a number, so the picker stays controlled from the
     first render. Empty means nothing is picked yet. */
  const [picked, setPicked] = useState("");
  const [quality, setQuality] = useState("");
  const [onlyApplied, setOnlyApplied] = useState(false);
  const [packsOpen, setPacksOpen] = useState(false);

  /* The client's champ-select pick is what the page follows, so the skin the
     reader wants is one click away instead of a search. */
  useEffect(() => {
    let alive = true;
    let seen = 0;

    async function follow() {
      const result = await api.cn.autoStatus();
      if (!alive || !isOk(result)) return;

      const champion = result.value.currentChampion;
      if (champion && champion !== seen) {
        seen = champion;
        setPicked(String(champion));
      }
      if (!champion) seen = 0;
    }

    void follow();
    const id = setInterval(() => void follow(), 2000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  const hero: CnHero | null =
    heroes.data?.heroes.find((candidate) => String(candidate.id) === picked) ?? null;

  /* The skin lives in the overlay, which the injection host serves to the game.
     The host scans for the game, so it starts first and the launch follows. */
  async function launch() {
    if (isBusy) return;

    try {
      unwrap(await api.cn.startPatcher());
    } catch (error: unknown) {
      const detail = errorMessage(error);

      /* A host this page started earlier is already doing the job. Every other
         refusal means the game would run without an overlay, so it stops the
         launch rather than hiding behind "League is already running". */
      if (!detail.includes("the patcher is already running")) {
        toast.error(m.cn_inject_failed_title(), detail);
        return;
      }
    }

    /* A host that was already up may be one that failed to hook. Starting the
       game over it would show the same nothing the reader just reported. */
    const host = await api.cn.patcherStatus();
    if (isOk(host) && host.value.error) {
      toast.error(m.cn_inject_failed_title(), host.value.error);
      return;
    }

    await launchOnly();
    toast.info(m.cn_load_started_title(), m.cn_load_started_description());
  }

  if (heroes.isPending) {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-meta text-surface-400">
        <Spinner size="sm" />
        {m.cn_heroes_loading()}
      </div>
    );
  }

  if (heroes.isError) {
    const copy = describeError(heroes.error);

    return (
      <EmptyState
        title={m.cn_heroes_error_title()}
        description={copy.detail ?? copy.description ?? copy.title}
      />
    );
  }

  if (heroes.data.heroes.length === 0) {
    return <EmptyState title={m.cn_heroes_empty()} />;
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-3 overflow-hidden p-6" data-ui="CnCatalog">
      <header className="flex shrink-0 items-baseline gap-3">
        <h1 className="text-lg font-medium text-surface-100">{m.cn_title()}</h1>
        <span className="text-fine text-surface-500">
          {m.cn_patch_label({ version: heroes.data.version })}
        </span>
      </header>

      <div
        className="flex shrink-0 items-center gap-2 rounded-md border border-surface-800 bg-surface-900/40 px-3 py-1.5"
        data-ui="CnCatalog:status-bar"
      >
        <CnSessionControl />
      </div>

      <p className="shrink-0 text-fine text-surface-500">{m.cn_risk_notice()}</p>

      <div className="flex min-h-0 flex-1 flex-col gap-4 lg:flex-row">
        <aside className="flex max-h-56 min-h-0 flex-col lg:max-h-none lg:w-60 lg:shrink-0">
          <CnHeroPicker
            heroes={heroes.data.heroes}
            picked={picked}
            onPick={(value) => setPicked(value)}
          />
        </aside>

        <section className="flex min-h-0 flex-1 flex-col gap-2">
          <div className="flex shrink-0 flex-wrap items-center gap-2">
            <ComboboxField
              className="w-40"
              placeholder={m.cn_quality_filter_placeholder()}
              options={qualityOptions()}
              value={quality}
              onValueChange={(value) => setQuality(value ?? "")}
            />

            <div className="flex items-center gap-2">
              <Switch
                id="cn-only-applied"
                checked={onlyApplied}
                onCheckedChange={(checked) => setOnlyApplied(checked)}
              />
              <label htmlFor="cn-only-applied" className="text-sm text-surface-300">
                {m.cn_only_applied_filter()}
              </label>
            </div>

            <Button size="sm" variant="ghost" onClick={() => setPacksOpen((open) => !open)}>
              {m.cn_packs_title()}
            </Button>

            <Button
              size="sm"
              className="ml-auto"
              disabled={isBusy}
              loading={isBusy}
              onClick={launch}
            >
              {m.cn_load_action()}
            </Button>
          </div>

          {/* Packs are page-level rather than champion-level: one carries the archives it names,
              whichever champion that is. Folded away so the skins keep the height. */}
          {packsOpen && <CnSkinPacks />}

          <div className="min-h-0 flex-1 overflow-y-auto pr-1">
            <CnSkinGrid
              hero={hero}
              quality={quality === "" ? null : (quality as CnQuality)}
              onlyApplied={onlyApplied}
            />
          </div>
        </section>
      </div>
    </div>
  );
}
