pub fn select_from_till_fill_size<
    'i,
    const SIZE: usize,
    T,
    M: FnMut(&T) -> usize + 'i,
    I: Iterator<Item = T> + 'i,
>(
    mut measure: M,
    items: I,
) -> impl Iterator<Item = T> + 'i {
    let mut current_size = 0usize;
    items.take_while(move |item: &T| {
        current_size += measure(item);
        current_size <= SIZE
    })
}
