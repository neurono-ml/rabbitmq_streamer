pub fn make_routing_key(app_group_namespace: &str, queue_name: &str) -> String {
    format!("{}.{}", app_group_namespace, queue_name)
}