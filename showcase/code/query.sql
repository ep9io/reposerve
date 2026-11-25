-- Files a reader opens most, to show SQL colouring.

with opens as (
    select path, count(*) as views
    from access
    where day >= current_date - interval '30 days'
    group by path
)
select
    f.path,
    f.extension,
    o.views,
    round(f.size / 1024.0, 1) as size_kib
from opens as o
join files as f using (path)
where o.views > 5
order by o.views desc, f.path
limit 25;
