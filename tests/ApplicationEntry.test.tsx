import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { ApplicationEntry } from "../src/ApplicationEntry";

const state = vi.hoisted(()=>({ desktop:false, config:vi.fn() }));
vi.mock("../src/App",()=>({ App:()=> <div>administrator</div> }));
vi.mock("../src/lib/transport",()=>({ isDesktop:()=>state.desktop }));
vi.mock("../src/saas/entry",()=>({ SaasPortal:()=> <div>user portal</div> }));
vi.mock("../src/saas/api",()=>({ createUserClient:()=>({publicConfig:state.config}) }));
beforeEach(()=>{state.desktop=false;state.config.mockReset().mockResolvedValue({enabled:true});window.history.replaceState({},"","/");});
afterEach(cleanup);

it("keeps bundled desktop and fixed admin route in the administrator app",()=>{
  state.desktop=true;
  const first=render(<ApplicationEntry />);
  expect(screen.getByText("administrator")).toBeInTheDocument();
  expect(state.config).not.toHaveBeenCalled();
  first.unmount();state.desktop=false;window.history.replaceState({},"","/ai-switch-admin");
  render(<ApplicationEntry />);
  expect(screen.getByText("administrator")).toBeInTheDocument();
  expect(state.config).not.toHaveBeenCalled();
});
it("serves the user portal at enabled web root without mounting administrator code",async()=>{
  render(<ApplicationEntry />);
  expect(await screen.findByText("user portal")).toBeInTheDocument();
  expect(screen.queryByText("administrator")).not.toBeInTheDocument();
});
it("replaces the disabled web root with the reserved admin path",async()=>{
  state.config.mockResolvedValue({enabled:false});
  render(<ApplicationEntry />);
  await waitFor(()=>expect(window.location.pathname).toBe("/ai-switch-admin"));
  expect(screen.getByText("administrator")).toBeInTheDocument();
});
