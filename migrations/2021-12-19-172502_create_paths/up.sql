CREATE TABLE paths (
    id                text primary key not null,
    path              text not null,
    registration_time unsigned bigint,
    last_accessed     unsigned bigint,
    nar_size          unsigned int not null,
    nar_hash          text not null,
    file_size         unsigned int,
    file_hash         text,
    url               text,
    compression       text,
    deriver           text,
    ca                text,
    sigs              text not null,
    refs              text not null
)
