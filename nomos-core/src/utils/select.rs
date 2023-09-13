pub fn select_from_till_fill_size<'i, const SIZE: usize, T>(
    mut measure: impl FnMut(&T) -> usize + 'i,
    items: impl Iterator<Item = T> + 'i,
) -> Box<dyn Iterator<Item = T> + 'i> {
    let mut current_size = 0usize;
    Box::new(items.take_while(move |tx: &T| {
        current_size += measure(tx);
        current_size <= SIZE
    }))
}
