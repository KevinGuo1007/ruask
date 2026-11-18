import type { ReactNode } from "react"
import { motion, useReducedMotion } from "motion/react"
import { ArrowRightIcon } from "lucide-react"

import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion"
import { Badge } from "@/components/ui/badge"
import { Button, Kbd } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"
import { AsciiWordmark } from "@/components/site/ascii-wordmark"
import { Cross, Display, Kicker, Reveal, Section } from "@/components/site/blueprint"
import { CodeBlock, CopyButton } from "@/components/site/code-block"
import { FlowDiagram } from "@/components/site/flow-diagram"
import { GithubMark, Nav } from "@/components/site/nav"
import { Terminal } from "@/components/site/terminal"
import { VideoFrame } from "@/components/site/video-frame"
import { INSTALL, REPO } from "@/content"
import { PrefsProvider, usePrefs } from "@/hooks/use-prefs"

const EASE = [0.2, 0.7, 0.1, 1] as const

function Hero() {
  const { t, lang } = usePrefs()
  const reduce = useReducedMotion()
  const line = (text: string, delay: number, className?: string) => (
    <span className="block overflow-hidden pb-[0.08em]">
      <motion.span
        key={`${lang}-${text}`}
        className={`block ${className ?? ""}`}
        initial={reduce ? false : { y: "105%" }}
        animate={{ y: 0 }}
        transition={{ duration: 0.9, delay, ease: EASE }}
      >
        {text}
      </motion.span>
    </span>
  )
  return (
    <section id="top" className="relative overflow-hidden">
      <div
        aria-hidden
        className="bg-blueprint-grid absolute inset-0 [mask-image:radial-gradient(ellipse_70%_80%_at_70%_30%,#000_30%,transparent_75%)]"
      />
      <div className="relative mx-auto grid max-w-[1180px] items-center gap-12 border-x border-dashed border-rule px-5 pt-16 pb-20 sm:px-10 md:pt-24 lg:grid-cols-[minmax(0,5fr)_minmax(0,6fr)] lg:gap-14">
        <div>
          <Reveal y={6}>
            <Badge variant="terminal">{t.hero.kicker}</Badge>
          </Reveal>
          <Display as="h1" className="mt-6 text-[clamp(2.2rem,4.4vw,3.5rem)] leading-[1.1] text-pretty [word-break:keep-all]">
            {line(t.hero.title1, 0.05)}
            {line(t.hero.title2, 0.18, "text-blueprint italic")}
          </Display>
          <Reveal delay={0.35}>
            <p className="mt-6 max-w-[34em] text-[16px] leading-[1.75] text-ink-muted">{t.hero.lead}</p>
          </Reveal>
          <Reveal delay={0.45} className="mt-8">
            <div className="mb-2 font-mono text-[11px] text-ink-faint">{t.hero.install}</div>
            <CodeBlock lines={INSTALL} />
            <p className="mt-2 text-[13px] text-ink-faint">{t.hero.installHint}</p>
          </Reveal>
          <Reveal delay={0.55} className="mt-8 flex flex-wrap items-center gap-3">
            <Button size="lg" asChild>
              <a href="#start">
                {t.hero.start}
                <ArrowRightIcon className="transition-transform duration-200 group-hover/button:translate-x-0.5" />
              </a>
            </Button>
            <Button size="lg" variant="outline" asChild>
              <a href={REPO}>
                <GithubMark className="size-4" />
                {t.hero.github}
                <Kbd>↗</Kbd>
              </a>
            </Button>
          </Reveal>
        </div>
        <motion.div
          initial={reduce ? false : { opacity: 0, y: 24 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.9, delay: 0.3, ease: EASE }}
          className="min-w-0"
        >
          <Terminal />
          <p className="mt-3 font-mono text-[11px] leading-relaxed text-ink-faint">{t.hero.termNote}</p>
        </motion.div>
      </div>
    </section>
  )
}

function Stats() {
  const { t } = usePrefs()
  return (
    <section className="relative border-t border-rule">
      <div className="relative mx-auto max-w-[1180px] border-x border-dashed border-rule">
        <Cross className="top-0 left-0" />
        <Cross className="top-0 left-full" />
        <div className="px-5 pt-12 pb-6 sm:px-10">
          <AsciiWordmark />
        </div>
        <div className="grid grid-cols-2 border-t border-rule lg:grid-cols-4">
          {t.stats.map((s, i) => (
            <Reveal
              key={s.label}
              delay={i * 0.08}
              className="border-rule px-5 py-6 sm:px-10 [&:nth-child(-n+2)]:border-b lg:[&:nth-child(-n+2)]:border-b-0 [&:nth-child(odd)]:border-r lg:[&:not(:last-child)]:border-r"
            >
              <div className="font-serif text-[2rem] leading-none font-light italic">{s.value}</div>
              <div className="mt-2 text-[13px] leading-snug text-ink-muted">{s.label}</div>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  )
}

function Video() {
  const { t } = usePrefs()
  return (
    <Section id="video">
      <div className="grid gap-10 lg:grid-cols-[minmax(0,1fr)_minmax(0,2.6fr)] lg:gap-14">
        <div>
          <Kicker index="01">{t.video.kicker}</Kicker>
          <Display className="mt-5 text-[clamp(1.8rem,3.2vw,2.6rem)] leading-tight">{t.video.title}</Display>
          <p className="mt-4 text-ink-muted">{t.video.caption}</p>
        </div>
        <Reveal>
          <VideoFrame label={t.video.title} />
        </Reveal>
      </div>
    </Section>
  )
}

function How() {
  const { t } = usePrefs()
  return (
    <Section id="how" className="bg-paper-sunken/40">
      <Kicker index="02">{t.how.kicker}</Kicker>
      <div className="mt-5 grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.3fr)] lg:items-end">
        <Display className="text-[clamp(1.8rem,3.2vw,2.6rem)] leading-tight">{t.how.title}</Display>
        <p className="text-ink-muted">{t.how.lead}</p>
      </div>
      <div className="mt-12">
        <FlowDiagram />
      </div>
    </Section>
  )
}

function Features() {
  const { t } = usePrefs()
  return (
    <Section id="features">
      <Kicker index="03">{t.features.kicker}</Kicker>
      <Display className="mt-5 text-[clamp(1.8rem,3.2vw,2.6rem)] leading-tight">{t.features.title}</Display>
      <div className="mt-12 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {t.features.items.map((f, i) => (
          <Reveal key={f.t} delay={(i % 4) * 0.07} className="h-full">
            <Card className="h-full transition-colors duration-300 hover:bg-blueprint-soft/50">
              <CardHeader>
                <div className="font-mono text-[11px] text-ink-faint transition-colors group-hover/card:text-blueprint">
                  {String(i + 1).padStart(2, "0")}
                </div>
                <CardTitle className="mt-6">{f.t}</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>{f.d}</CardDescription>
              </CardContent>
            </Card>
          </Reveal>
        ))}
      </div>
    </Section>
  )
}

const CONFIG = (
  <>
    <span className="text-term-dim"># ~/.config/ruask/config.toml</span>
    {"\n"}
    <span className="text-term-cmd">api_type</span> = <span className="text-term-use">"openai"</span>
    <span className="text-term-dim">   # or "anthropic"</span>
    {"\n"}
    <span className="text-term-dim"># language = "zh"   # en / zh</span>
    {"\n\n"}
    <span className="text-term-title">[openai]</span>
    {"\n"}
    <span className="text-term-cmd">base_url</span> = <span className="text-term-use">"https://your-provider.example/v1"</span>
    {"\n"}
    <span className="text-term-cmd">model</span>    = <span className="text-term-use">"your-model"</span>
    {"\n"}
    <span className="text-term-cmd">api_key</span>  = <span className="text-term-use">"YOUR_API_KEY"</span>
    {"\n\n"}
    <span className="text-term-title">[anthropic]</span>
    {"\n"}
    <span className="text-term-cmd">base_url</span> = <span className="text-term-use">"https://your-anthropic-provider.example/v1"</span>
    {"\n"}
    <span className="text-term-cmd">model</span>    = <span className="text-term-use">"your-model"</span>
    {"\n"}
    <span className="text-term-cmd">api_key</span>  = <span className="text-term-use">"YOUR_API_KEY"</span>
  </>
)

function QuickStart() {
  const { t, lang } = usePrefs()
  const [s1, s2, s3, s4] = t.start.steps
  const step = (n: number, s: { t: string; d: string }, body: ReactNode) => (
    <Reveal className="relative grid gap-4 pb-12 pl-12 last:pb-0 md:grid-cols-[minmax(0,1fr)_minmax(0,1.35fr)] md:gap-10">
      <span className="absolute top-0 left-0 grid size-8 place-items-center border border-rule-strong bg-background font-mono text-[12px] text-blueprint">
        {n}
      </span>
      <div>
        <h3 className="text-[17px] font-medium">{s.t}</h3>
        <p className="mt-2 text-[14px] text-ink-muted">{s.d}</p>
      </div>
      <div className="min-w-0">{body}</div>
    </Reveal>
  )
  return (
    <Section id="start">
      <Kicker index="04">{t.start.kicker}</Kicker>
      <Display className="mt-5 text-[clamp(1.8rem,3.2vw,2.6rem)] leading-tight">{t.start.title}</Display>
      <div className="relative mt-12">
        <span aria-hidden className="rule-dashed-y absolute top-8 bottom-8 left-4" />
        {step(1, s1, <CodeBlock lines={INSTALL} />)}
        {step(2, s2, <CodeBlock lines={["ruask --update-pages", "ruask --update-pages --from ./tldr.zip"]} />)}
        {step(
          3,
          s3,
          <Tabs defaultValue="file">
            <TabsList variant="line">
              <TabsTrigger value="file">{t.start.cfgFile}</TabsTrigger>
              <TabsTrigger value="env">{t.start.cfgEnv}</TabsTrigger>
            </TabsList>
            <TabsContent value="file" className="mt-3 grid gap-3">
              <CodeBlock lines={["ruask --init-config"]} />
              <CodeBlock lines={CONFIG} />
            </TabsContent>
            <TabsContent value="env" className="mt-3 grid gap-3">
              <CodeBlock
                lines={[
                  'export OPENAI_BASE_URL="https://your-provider.example/v1"',
                  'export OPENAI_API_KEY="YOUR_API_KEY"',
                  'export OPENAI_MODEL="your-model"',
                ]}
              />
              <p className="text-[13px] text-ink-faint">{t.start.envNote}</p>
            </TabsContent>
          </Tabs>
        )}
        {step(4, s4, <CodeBlock lines={lang === "zh" ? ["ruask tar", 'ruask git commit -q "如何修改上一次提交的信息"'] : ["ruask tar", 'ruask git commit -q "how do I edit the last commit message"']} />)}
      </div>
    </Section>
  )
}

function Examples() {
  const { t } = usePrefs()
  return (
    <Section id="examples" className="bg-paper-sunken/40">
      <Kicker index="05">{t.examples.kicker}</Kicker>
      <Display className="mt-5 text-[clamp(1.8rem,3.2vw,2.6rem)] leading-tight">{t.examples.title}</Display>
      <ul className="mt-12 grid border-t border-rule md:grid-cols-2">
        {t.examples.items.map((e, i) => (
          <Reveal
            key={e.c}
            delay={(i % 2) * 0.06}
            className="group/row flex items-start gap-3 border-b border-rule py-4 md:px-4 md:[&:nth-child(odd)]:border-r md:[&:nth-child(odd)]:pl-0"
          >
            <div className="min-w-0 flex-1">
              <code className="block font-mono text-[13px] break-words text-foreground">
                <span className="text-signal select-none">$ </span>
                {e.c}
              </code>
              <p className="mt-1 text-[13px] text-ink-faint">{e.d}</p>
            </div>
            <CopyButton
              text={e.c}
              className="text-ink-muted hover:bg-paper-sunken md:opacity-0 md:group-hover/row:opacity-100 md:focus-visible:opacity-100"
            />
          </Reveal>
        ))}
      </ul>
    </Section>
  )
}

function Faq() {
  const { t } = usePrefs()
  return (
    <Section id="scope">
      <div className="grid gap-10 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.6fr)]">
        <div>
          <Kicker index="06">{t.faq.kicker}</Kicker>
          <Display className="mt-5 text-[clamp(1.8rem,3.2vw,2.6rem)] leading-tight">{t.faq.title}</Display>
        </div>
        <Accordion type="single" collapsible defaultValue="0" className="border-t border-rule">
          {t.faq.items.map((f, i) => (
            <AccordionItem key={f.q} value={String(i)}>
              <AccordionTrigger>
                <span className="flex items-baseline gap-4">
                  <span className="font-mono text-[11px] text-ink-faint">{String(i + 1).padStart(2, "0")}</span>
                  {f.q}
                </span>
              </AccordionTrigger>
              <AccordionContent className="pl-9 text-[14px] leading-relaxed">{f.a}</AccordionContent>
            </AccordionItem>
          ))}
        </Accordion>
      </div>
    </Section>
  )
}

function Footer() {
  const { t } = usePrefs()
  return (
    <footer className="relative border-t border-rule">
      <div className="relative mx-auto max-w-[1180px] overflow-hidden border-x border-dashed border-rule px-5 pt-14 pb-10 sm:px-10">
        <Cross className="top-0 left-0" />
        <Cross className="top-0 left-full" />
        <Reveal y={30}>
          <div
            aria-hidden
            className="font-serif text-[clamp(5rem,20vw,15rem)] leading-[0.85] font-light tracking-[-0.04em] text-transparent italic [-webkit-text-stroke:1px_var(--ink-faint)] opacity-60"
          >
            ruask
          </div>
        </Reveal>
        <div className="mt-10 flex flex-wrap items-center gap-x-6 gap-y-3">
          <span className="font-mono text-[13px]">
            <span className="text-signal">❯</span> ruask <span className="text-ink-faint">v0.1.0</span>
          </span>
          <Button variant="bracket" asChild>
            <a href={REPO}>GitHub</a>
          </Button>
          <Button variant="bracket" asChild>
            <a href={`${REPO}/issues`}>Issues</a>
          </Button>
          <Button variant="bracket" asChild>
            <a href={`${REPO}#readme`}>README</a>
          </Button>
          <Button variant="bracket" asChild>
            <a href="https://github.com/tldr-pages/tldr">tldr-pages</a>
          </Button>
        </div>
        <p className="mt-6 max-w-3xl text-[12.5px] leading-relaxed text-ink-faint">
          {t.footer.license} {t.footer.tldr}
        </p>
      </div>
    </footer>
  )
}

function Page() {
  const { t } = usePrefs()
  return (
    <>
      <a
        href="#main"
        className="fixed top-2 left-2 z-[60] -translate-y-20 bg-foreground px-3 py-1.5 font-mono text-xs text-background focus:translate-y-0"
      >
        {t.skip}
      </a>
      <Nav />
      <main id="main">
        <Hero />
        <Stats />
        <Video />
        <How />
        <Features />
        <QuickStart />
        <Examples />
        <Faq />
      </main>
      <Footer />
      <Toaster position="bottom-center" />
    </>
  )
}

export default function App() {
  return (
    <PrefsProvider>
      <TooltipProvider delayDuration={300}>
        <Page />
      </TooltipProvider>
    </PrefsProvider>
  )
}
