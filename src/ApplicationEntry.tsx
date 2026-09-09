import { useEffect, useState } from "react";
import { App } from "./App";
import { isDesktop } from "./lib/transport";
import { createUserClient } from "./saas/api";
import { SaasPortal } from "./saas/entry";

export function ApplicationEntry() {
  const [mode,setMode] = useState<"admin"|"loading"|"portal"|"error">(()=>
    isDesktop() || window.location.pathname==="/ai-switch-admin" || window.location.pathname.startsWith("/ai-switch-admin/") ? "admin" : "loading");
  useEffect(()=>{
    if (mode!=="loading") return;
    let disposed=false;
    void createUserClient().publicConfig().then(config=>{
      if (disposed) return;
      if (config.enabled) setMode("portal");
      else { window.history.replaceState({},"","/ai-switch-admin");setMode("admin"); }
    }).catch(()=>{ if(!disposed) setMode("error"); });
    return ()=>{disposed=true;};
  },[mode]);
  if(mode==="admin") return <App />;
  if(mode==="portal") return <SaasPortal />;
  return <main className="grid min-h-screen place-content-center gap-4 p-8 text-center text-stone-700">
    <h1 className="text-xl font-semibold">AI Switch</h1>
    {mode==="error" ? <><p role="alert">无法连接站点 / Unable to load this site.</p><button type="button" onClick={()=>setMode("loading")}>重试 / Retry</button></> : <p role="status">正在加载 / Loading…</p>}
  </main>;
}
