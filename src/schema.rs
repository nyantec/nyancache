table! {
    paths (id) {
        id -> Text,
        path -> Text,
        registration_time -> Nullable<BigInt>,
        last_accessed -> Nullable<BigInt>,
        nar_size -> Integer,
        nar_hash -> Text,
        file_size -> Nullable<Integer>,
        file_hash -> Nullable<Text>,
        url -> Nullable<Text>,
        compression -> Nullable<Text>,
        deriver -> Nullable<Text>,
        ca -> Nullable<Text>,
        sigs -> Text,
        refs -> Text,
    }
}
