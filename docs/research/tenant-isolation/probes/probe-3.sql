\set ON_ERROR_STOP 0
set role app;
\echo '--- matview over RLS table (owner not forced) shows all tenants'
begin; select set_config('app.tenant_id','1',true); select count(*) from m.nf_dummy_never; rollback;
begin; select set_config('app.tenant_id','1',true); select count(*) as mv2_rows from m.mv2; rollback;
begin; select set_config('app.tenant_id','3',true); set local max_parallel_workers_per_gather=0;
select count(*) from m.big; select count(*) from m.big2;
\timing on
select count(*) from m.big; select count(*) from m.big2; select count(*) from m.big; select count(*) from m.big2; select count(*) from m.big; select count(*) from m.big2;
\timing off
rollback;
reset role;
\echo '--- policies on partitions: alter table parent recursion?'
alter table m.pt force row level security;
select relname, relrowsecurity, relforcerowsecurity from pg_class where relname in ('pt','pt0');
