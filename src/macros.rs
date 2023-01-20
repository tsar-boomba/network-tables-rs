macro_rules! cfg_tracing {
	($($item:item)*) => {
        #[cfg(feature = "tracing")]
        {
            $(
                $item
            )*
        }
    }
}
