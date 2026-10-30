# Rowspan table

A Markdown table with wrapped cells, each line aligned per column:

| Left | Center | Right |
|:-----|:------:|------:|
| A short cell | This centered cell is long enough to wrap onto a second and a third line inside its column | 12.50 |
| Second row | Short | A right-aligned cell that also wraps onto more than one line |

An HTML table with `rowspan`:

<table>
<tr><th>Group</th><th>Item</th><th align="right">Price</th></tr>
<tr><td rowspan="3">Fruit, a cell that spans three rows and wraps onto several lines</td><td>Apple</td><td align="right">1.20</td></tr>
<tr><td>Banana</td><td align="right">0.80</td></tr>
<tr><td>Cherry, a longer item name that wraps</td><td align="right">4.00</td></tr>
<tr><td>Bread</td><td>Rye</td><td align="right">2.10</td></tr>
</table>

End.
