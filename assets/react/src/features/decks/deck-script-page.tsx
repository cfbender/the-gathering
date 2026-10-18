import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { BookOpen, Check, FileCode2, RotateCcw, Save, Undo2 } from "lucide-react"
import { useEffect, useRef, useState } from "react"
import { PageHeader } from "@/components/app-shell"
import { Button } from "@/components/ui/button"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { ApiError } from "@/lib/api"
import { getDeckScript, previewDeckScript, saveDeckScript } from "@/features/decks/deck-chooser"
import { DeckScriptEditor, type DeckScriptEditorHandle } from "./deck-script-editor"
import { DECK_SCRIPT_EXAMPLES, STARTER_SCRIPT } from "./deck-script-examples"
import { ContextPanel, PreviewPanel, ReferencePanel } from "./deck-script-panels"

const PREVIEW_DELAY_MS = 400

export function DeckScriptPage() {
  const script = useQuery({ queryKey: ["deck-chooser", "script"], queryFn: getDeckScript })
  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Deck chooser"
        title="Customize the chooser"
        description="Write a Lua script that weighs each of your decks. The preview runs your draft against your real decks and history as you type, so you can see every deck's chance before you save."
      />
      <a href="/decks/choose" className="link link-hover text-base-content/65 w-fit text-sm">
        ← Back to the chooser
      </a>
      {script.isPending && <span className="loading loading-spinner loading-lg mx-auto my-16" />}
      {script.isError && (
        <div role="alert" className="alert alert-error">
          Could not load your script. Try again.
        </div>
      )}
      {script.isSuccess && <ScriptWorkbench saved={script.data} />}
    </div>
  )
}

function lineOf(message: string | undefined) {
  const match = message ? /^line (\d+):/.exec(message) : null
  return match?.[1] ? Number(match[1]) : null
}

function ScriptWorkbench({ saved }: { saved: string | null }) {
  const queryClient = useQueryClient()
  const editor = useRef<DeckScriptEditorHandle>(null)
  const [draft, setDraft] = useState(saved ?? STARTER_SCRIPT)
  const [debounced, setDebounced] = useState(draft)
  const [confirmReset, setConfirmReset] = useState(false)
  const [justSaved, setJustSaved] = useState(false)
  const baseline = saved ?? STARTER_SCRIPT
  const dirty = draft !== baseline

  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(draft), PREVIEW_DELAY_MS)
    return () => window.clearTimeout(timer)
  }, [draft])

  const preview = useQuery({
    queryKey: ["deck-chooser", "preview", debounced],
    queryFn: () => previewDeckScript(debounced),
    placeholderData: keepPreviousData,
  })

  const save = useMutation({
    mutationFn: saveDeckScript,
    onSuccess: (script) => {
      queryClient.setQueryData(["deck-chooser", "script"], script)
      // Picks and previews depend on the saved script.
      void queryClient.invalidateQueries({
        queryKey: ["deck-chooser"],
        predicate: (query) => query.queryKey[1] !== "script",
      })
      setDraft(script ?? STARTER_SCRIPT)
      setJustSaved(true)
    },
  })
  const saveError = save.error instanceof ApiError ? save.error.fieldErrors("script")[0] : undefined
  const saveErrorLine = lineOf(saveError)
  const errorLine = saveErrorLine ?? preview.data?.error?.line ?? null

  function edit(value: string) {
    setDraft(value)
    setJustSaved(false)
    if (save.isError) save.reset()
  }

  return (
    <div className="grid items-start gap-6 xl:grid-cols-2">
      <section className="flex min-w-0 flex-col gap-3" aria-label="Script editor">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-base-content/70 mr-auto text-sm">
            Picks use{" "}
            <strong>{saved === null ? "the built-in weights" : "your saved script"}</strong>
            {dirty && <span className="badge badge-warning badge-sm ml-2">Unsaved changes</span>}
            {justSaved && !dirty && (
              <span className="text-success ml-2 inline-flex items-center gap-1 text-xs">
                <Check className="size-3.5" /> Saved
              </span>
            )}
          </span>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button size="sm" variant="outline">
                <FileCode2 className="size-4" /> Examples
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent className="max-h-[var(--radix-dropdown-menu-content-available-height)] w-80 overflow-y-auto">
              {DECK_SCRIPT_EXAMPLES.map((example) => (
                <DropdownMenuItem
                  key={example.title}
                  className="flex-col items-start gap-0.5"
                  onSelect={() => edit(example.source)}
                >
                  <span>{example.title}</span>
                  <span className="text-base-content/60 text-xs font-normal">
                    {example.description}
                  </span>
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
          {dirty && (
            <Button size="sm" variant="ghost" onClick={() => edit(baseline)}>
              <Undo2 className="size-4" /> Discard changes
            </Button>
          )}
          <Button size="sm" onClick={() => save.mutate(draft)} disabled={!dirty || save.isPending}>
            <Save className="size-4" /> {save.isPending ? "Saving…" : "Save script"}
          </Button>
        </div>
        <DeckScriptEditor
          ref={editor}
          label="Deck chooser script"
          value={draft}
          onChange={edit}
          errorLine={errorLine}
        />
        {saveError && (
          <div role="alert" className="alert alert-error">
            <span className="min-w-0 font-mono text-xs break-words">
              Could not save: {saveError}
            </span>
            {saveErrorLine !== null && (
              <Button
                size="sm"
                variant="outline"
                onClick={() => editor.current?.goToLine(saveErrorLine)}
              >
                Line {saveErrorLine}
              </Button>
            )}
          </div>
        )}
        {save.isError && !saveError && (
          <div role="alert" className="alert alert-error">
            Could not save your script. Try again.
          </div>
        )}
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-base-content/55 text-xs">
            Lua 5.4 with <code>string</code>, <code>table</code>, <code>math</code>, and{" "}
            <code>utf8</code>. Tab indents; Shift+Tab outdents.
          </p>
          {saved !== null && (
            <Button size="sm" variant="ghost" onClick={() => setConfirmReset(true)}>
              <RotateCcw className="size-4" /> Use built-in weights
            </Button>
          )}
        </div>
        <ConfirmDialog
          open={confirmReset}
          onOpenChange={setConfirmReset}
          title="Go back to the built-in weights?"
          confirmLabel="Delete script"
          destructive
          onConfirm={() => save.mutate(null)}
        >
          This deletes your saved script. Picks will favor decks you have not played lately, as they
          do for everyone without a script.
        </ConfirmDialog>
      </section>

      <section className="min-w-0" aria-label="Live preview">
        <Tabs defaultValue="preview">
          <TabsList className="mb-4">
            <TabsTrigger value="preview">Preview</TabsTrigger>
            <TabsTrigger value="context">Context</TabsTrigger>
            <TabsTrigger value="reference">
              <BookOpen className="size-4" /> Reference
            </TabsTrigger>
          </TabsList>
          <TabsContent value="preview">
            {preview.isPending && <span className="loading loading-spinner" />}
            {preview.isError && (
              <div role="alert" className="alert alert-error">
                Could not run the preview. Try again.
              </div>
            )}
            {preview.data && (
              <PreviewPanel
                preview={preview.data}
                updating={preview.isFetching || draft !== debounced}
                onGoToLine={(line) => editor.current?.goToLine(line)}
              />
            )}
          </TabsContent>
          <TabsContent value="context">
            {preview.data && <ContextPanel preview={preview.data} />}
          </TabsContent>
          <TabsContent value="reference">
            <ReferencePanel />
          </TabsContent>
        </Tabs>
      </section>
    </div>
  )
}
