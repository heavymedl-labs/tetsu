\set ON_ERROR_STOP 0
\set VERBOSITY verbose
set role owner_r;
-- single-column FK target (global unique id)
create table m.s (tenant_id bigint not null, id bigint primary key);
create table m.sc (tenant_id bigint not null, id bigint primary key, s_id bigint references m.s(id) on delete cascade);
alter table m.s enable row level security; alter table m.s force row level security;
alter table m.sc enable row level security; alter table m.sc force row level security;
create policy p on m.s using (tenant_id = nullif(current_setting('app.tenant_id', true), '')::bigint);
create policy p on m.sc using (tenant_id = nullif(current_setting('app.tenant_id', true), '')::bigint);
grant select, insert, update, delete on m.s, m.sc to app;
create index on m.t (lower(email));
create materialized view m.mv as select * from m.t; -- created under FORCE as owner, no tenant: 0 rows
grant select on m.mv to app;
create table m.nf (tenant_id bigint, secret text); alter table m.nf enable row level security;
create policy p on m.nf using (tenant_id = nullif(current_setting('app.tenant_id', true), '')::bigint);
insert into m.nf values (1,'a'),(2,'b');
create function m.peek() returns bigint language sql security definer set search_path = '' as $$ select count(*) from m.nf $$;
grant execute on function m.peek() to app;
create materialized view m.mv2 as select * from m.nf;
grant select on m.mv2 to app;
-- partitions
create table m.pt (tenant_id bigint not null, id bigint, primary key (tenant_id,id)) partition by hash (tenant_id);
create table m.pt0 partition of m.pt for values with (modulus 2, remainder 0);
create table m.pt1 partition of m.pt for values with (modulus 2, remainder 1);
alter table m.pt enable row level security;
create policy p on m.pt using (tenant_id = nullif(current_setting('app.tenant_id', true), '')::bigint);
insert into m.pt select g, g from generate_series(1,10) g;
grant select on m.pt, m.pt0 to app;
select relname, relrowsecurity, relforcerowsecurity from pg_class where relname in ('pt','pt0','pt1');
reset role;
insert into m.s values (1,1),(2,2);
insert into m.sc values (2,20,2);
set role app;
begin; select set_config('app.tenant_id','1',true);
\echo '--- FK to another tenant row (single-column FK): succeeds = cross-tenant reference + existence oracle'
insert into m.sc values (1,10,2);
\echo '--- FK to nonexistent row'
insert into m.sc values (1,11,999);
\echo '--- delete own parent row 1 is fine; check cascade from tenant 1 parent into tenant 2 child'
rollback;
begin; select set_config('app.tenant_id','1',true);
insert into m.sc values (1,10,2);
commit;
reset role;
\echo '--- now tenant 2 deletes its parent row 2: cascade removes tenant 1 child row it cannot see'
set role app;
begin; select set_config('app.tenant_id','2',true);
delete from m.s where id = 2;
rollback;
reset role; select * from m.sc; set role app;
\echo '--- composite FK to other tenant row'
begin; select set_config('app.tenant_id','1',true);
insert into m.c(tenant_id,parent_id) values (1, 2);
rollback;
\echo '--- unique violation detail for app (verbose)'
begin; select set_config('app.tenant_id','1',true);
insert into m.t(tenant_id,email) values (1,'u2@x');
rollback;
\echo '--- non-leakproof user qual (lower) with index on lower(email)'
begin; select set_config('app.tenant_id','1',true);
explain select * from m.t where lower(email) = 'u1@x';
explain select * from m.t where email = 'u1@x';
rollback;
\echo '--- security definer owned by non-FORCE owner sees all tenants'
begin; select set_config('app.tenant_id','1',true); select m.peek(); select count(*) from m.nf;
\echo '--- matview ignores RLS (no RLS on matviews), shows all rows of mv2'
select count(*) from m.mv2;
rollback;
\echo '--- direct partition access: no policy on pt0, parent policy ignored'
select count(*) from m.pt; select count(*) from m.pt0;
\echo '--- row_security off as app'
set row_security = off; select count(*) from m.t; reset row_security;
\echo '--- sequence leak: app can read identity sequence?'
select last_value from m.t_id_seq;
reset role;
\echo '--- leakproof status'
select proname, proleakproof from pg_proc where proname in ('current_setting','nullif','int8eq','texteq','lower','textlike') order by 1;
\echo '--- cost of current_setting per row: 2M rows no tenant index'
create table m.big (tenant_id bigint, v int); insert into m.big select g%10, g from generate_series(1,2000000) g;
alter table m.big enable row level security; alter table m.big force row level security; alter table m.big owner to owner_r;
create policy p on m.big using (tenant_id = nullif(current_setting('app.tenant_id', true), '')::bigint);
create table m.big2 (like m.big); insert into m.big2 select * from m.big; alter table m.big2 owner to owner_r;
alter table m.big2 enable row level security; alter table m.big2 force row level security;
create policy p on m.big2 using (tenant_id = (select nullif(current_setting('app.tenant_id', true), '')::bigint));
grant select on m.big, m.big2 to app;
vacuum analyze m.big; vacuum analyze m.big2;
set role app;
begin; select set_config('app.tenant_id','3',true);
explain (analyze, costs off, timing off) select count(*) from m.big;
explain (analyze, costs off, timing off) select count(*) from m.big2;
explain (analyze, costs off, timing off) select count(*) from m.big;
explain (analyze, costs off, timing off) select count(*) from m.big2;
explain (analyze, costs off, timing off) select count(*) from m.big where tenant_id = 3;
rollback;
