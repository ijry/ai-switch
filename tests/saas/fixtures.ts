import type { SaasConfig, SaasGroup, SaasKey, SaasUser, UserOverview } from "../../src/saas/types";

export const user: SaasUser = { id:"u1",login:"octocat",avatarUrl:"",status:"active",balanceMicros:12500000,frozenMicros:500000,createdAt:"2026-09-01T00:00:00Z" };
export const publicConfig = { enabled:true,registrationEnabled:true,passwordLoginEnabled:true,siteName:"Switch Cloud",publicBaseUrl:"https://cloud.example.com",githubLoginAvailable:true,exchangeRateMicros:7000000 };
export const config: SaasConfig = { ...publicConfig,enabled:false,githubClientId:"client-id",githubClientSecretConfigured:true,logs:{ queue:"memory",store:"file",retentionDays:30,maxRecords:10000,maxBytes:16777216,batchSize:100 } };
export const group: SaasGroup = { id:"g1",name:"Everyday Codex",platform:"codex",isInternal:false,isActive:false,configured:true,accountCount:2,availableAccountCount:2,multiplierMicros:1000000,models:[{ model:"gpt-test",upstreamModel:"gpt-test",inputPriceMicros:2000000,cachePriceMicros:200000,outputPriceMicros:8000000,imagePriceMicros:0 }],maxConcurrency:3,maxOutputTokens:16384,timeoutSeconds:120 };
export const key: SaasKey = { id:"k1",name:"Workstation",prefix:"sk-saas-abcd",groupId:"g1",groupName:group.name,status:"active",createdAt:"2026-09-01T00:00:00Z",expiresAt:null,limitMicros:null,spentMicros:0,lastUsedAt:null };
export const recharge = { id:"r1",userId:"u1",githubLogin:"octocat",amountCnyFen:7000,exchangeRateMicros:7000000,creditMicros:10000000,status:"pending",note:"Transfer reference 42",createdAt:"2026-09-07T12:00:00Z",reviewedAt:null,reviewNote:null };
export const overview: UserOverview = { balanceMicros:12500000,frozenMicros:500000,today:{ requestCount:4,inputTokens:100,cacheReadTokens:20,outputTokens:80,costMicros:120000 },month:{ requestCount:40,inputTokens:1000,cacheReadTokens:200,outputTokens:800,costMicros:1234567 },redemptionHistory:[] };
export const page = <Item,>(items: Item[] = []) => ({ items,total:items.length });
