\set ON_ERROR_STOP 0
select version();
create role owner_r; create role app login;
create schema m authorization owner_r;
grant usage on schema m to app;
set role owner_r;
create table m.t (tenant_id bigint not null, id bigint generated always as identity, email text, primary key (tenant_id, id), unique (tenant_id, email), unique(email));
create table m.c (tenant_id bigint not null, id bigint generated always as identity, parent_id bigint, primary key(tenant_id,id), foreign key (tenant_id, parent_id) references m.t(tenant_id,id) on delete cascade);
create table m.c2 (tenant_id bigint not null, id bigint primary key, t_id bigint references m.t_alt(id));
alter table m.t enable row level security; alter table m.c enable row level security;
create policy p on m.t using (tenant_id = nullif(current_setting('app.tenant_id', true), '')::bigint);
create policy p on m.c using (tenant_id = nullif(current_setting('app.tenant_id', true), '')::bigint);
insert into m.t(tenant_id,email) select g%500+1, 'u'||g||'@x' from generate_series(1,200000) g;
insert into m.c(tenant_id,parent_id) values (1, 500);
\echo '--- owner without FORCE sees all rows'
select count(*) from m.t;
alter table m.t force row level security;
\echo '--- owner with FORCE, no tenant set'
select count(*) from m.t;
grant select, insert, update, delete, truncate on m.t, m.c to app;
create view m.v as select * from m.t;
create view m.vi with (security_invoker=true) as select * from m.t;
grant select on m.v, m.vi to app;
reset role;
analyze m.t;
set role app;
\echo '--- app no tenant'
select count(*) from m.t;
begin; select set_config('app.tenant_id','1',true);
\echo '--- app tenant 1'
select count(*) from m.t;
\echo '--- view owner-rights (owner_r has FORCE so filtered); security_invoker'
select count(*) from m.v; select count(*) from m.vi;
\echo '--- explain policy uses index?'
explain select * from m.t where id = 5;
explain select * from m.t where email like 'u1%';
\echo '--- unique side channel'
insert into m.t(tenant_id,email) values (1,'u2@x');
rollback;
begin; select set_config('app.tenant_id','1',true);
\echo '--- insert for other tenant'
insert into m.t(tenant_id,email) values (2,'new@x');
rollback;
\echo '--- after commit/rollback, setting is'
select current_setting('app.tenant_id', true) is null as isnull, current_setting('app.tenant_id', true) = '' as isempty;
\echo '--- app can set tenant itself (no restriction on custom GUC)'
select set_config('app.tenant_id','2',false);
select count(*) from m.t;
reset app.tenant_id;
\echo '--- truncate ignores RLS'
begin; select set_config('app.tenant_id','1',true); truncate m.c; rollback;
\echo '--- pg_stats visible to app?'
select count(*) from pg_stats where schemaname='m' and tablename='t';
\echo '--- generic plan'
begin; select set_config('app.tenant_id','3',true);
set local plan_cache_mode = force_generic_plan;
prepare q(bigint) as select * from m.t where id = $1;
explain execute q(5);
select set_config('app.tenant_id','4',true);
execute q(4);
rollback;
