import SwiftUI

struct BookRowView: View {
    let book: Book
    let isSelected: Bool
    
    var body: some View {
        HStack {
            Text("\(book.totalAnnotations)")
                .monospacedDigit()
                .frame(width: 40, alignment: .trailing)
            
            VStack(alignment: .leading, spacing: 2) {
                Text(book.title)
                    .font(.system(size: 13))
                    .lineLimit(2)
                Text(book.author)
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)
            }
            
            Spacer()
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 8)
        .background(isSelected ? Color.accentColor.opacity(0.2) : Color.clear)
        .cornerRadius(4)
    }
}
